use std::io;
use std::path::{Path, PathBuf};

use files::{
    Disposition, FileError, FileStore, FsFileStore, ImageError, Kind, LinkError, MAX_SIDE, Signer,
    check_avatar, chunks_of, hex, hmac_sha256, limit, put_content, safe_headers, sniff, valid_key,
};
use futures::StreamExt;

fn scratch(name: &str) -> PathBuf {
    let dir = PathBuf::from(env!("CARGO_TARGET_TMPDIR"))
        .join("m43")
        .join(name);
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).expect("папка для теста создаётся");
    dir
}

fn temps(root: &Path) -> Vec<String> {
    let mut found = Vec::new();
    let mut stack = vec![root.to_path_buf()];
    while let Some(dir) = stack.pop() {
        let Ok(entries) = std::fs::read_dir(&dir) else {
            continue;
        };
        for entry in entries.flatten() {
            let path = entry.path();
            if path.is_dir() {
                stack.push(path);
            } else if path.to_string_lossy().ends_with(".tmp") {
                found.push(path.to_string_lossy().to_string());
            }
        }
    }
    found
}

pub fn png(width: u32, height: u32) -> Vec<u8> {
    let mut data = b"\x89PNG\r\n\x1a\n\0\0\0\x0dIHDR".to_vec();
    data.extend_from_slice(&width.to_be_bytes());
    data.extend_from_slice(&height.to_be_bytes());
    data.extend_from_slice(&[8, 6, 0, 0, 0, 0, 0, 0, 0]);
    data
}

fn jpeg(width: u16, height: u16) -> Vec<u8> {
    let mut data = vec![0xff, 0xd8, 0xff, 0xe0, 0x00, 0x10];
    data.extend_from_slice(b"JFIF\0\x01\x01\0\0\x01\0\x01\0\0");
    data.extend_from_slice(&[0xff, 0xc4, 0x00, 0x04, 0x00, 0x00]);
    data.extend_from_slice(&[0xff, 0xc0, 0x00, 0x11, 0x08]);
    data.extend_from_slice(&height.to_be_bytes());
    data.extend_from_slice(&width.to_be_bytes());
    data.extend_from_slice(&[3, 1, 0x22, 0, 2, 0x11, 1, 3, 0x11, 1]);
    data
}

fn webp(width: u32, height: u32) -> Vec<u8> {
    let mut data = b"RIFF\x24\0\0\0WEBPVP8X\x0a\0\0\0\0\0\0\0".to_vec();
    data.extend_from_slice(&(width - 1).to_le_bytes()[..3]);
    data.extend_from_slice(&(height - 1).to_le_bytes()[..3]);
    data
}

fn broken_after(first: Vec<u8>) -> files::Chunks {
    let pieces: Vec<io::Result<Vec<u8>>> = vec![
        Ok(first),
        Err(io::Error::new(
            io::ErrorKind::ConnectionReset,
            "клиент ушёл",
        )),
    ];
    Box::pin(futures::stream::iter(pieces))
}

#[test]
fn keys_are_plain_names() {
    for good in ["abc", "ab-cd_ef.png", &"a".repeat(128)] {
        assert!(valid_key(good), "«{good}» годится");
    }
    for bad in [
        "",
        "ab",
        ".hidden",
        "../etc",
        "a/b",
        "AB",
        "ключ",
        &"a".repeat(129),
    ] {
        assert!(!valid_key(bad), "«{bad}» не ключ");
    }
}

#[tokio::test]
async fn fs_store_writes_whole_files_only() {
    let root = scratch("store");
    let store = FsFileStore::new(&root);
    let size = store
        .put("hello-file", chunks_of(b"hello world".to_vec(), 4))
        .await
        .expect("файл пишется");
    assert_eq!(size, 11);
    assert_eq!(
        store.get("hello-file").await.expect("чтение"),
        Some(b"hello world".to_vec())
    );
    assert!(
        root.join("he").join("hello-file").exists(),
        "файл лежит в подпапке по первым двум символам ключа"
    );
    let failed = store
        .put("broken-file", broken_after(b"half".to_vec()))
        .await;
    assert!(failed.is_err(), "оборванный поток это ошибка");
    assert!(
        !store.exists("broken-file").await.expect("проверка"),
        "недописанный файл не виден под ключом"
    );
    assert!(
        temps(&root).is_empty(),
        "страж Drop убрал временный файл: {:?}",
        temps(&root)
    );
    assert!(store.put("../x", chunks_of(vec![1], 1)).await.is_err());
    assert!(store.remove("hello-file").await.expect("удаление"));
    assert!(
        !store
            .remove("hello-file")
            .await
            .expect("повторное удаление")
    );
    assert_eq!(store.get("hello-file").await.expect("чтение"), None);
}

#[tokio::test]
async fn content_key_is_sha256() {
    let root = scratch("content");
    let store = FsFileStore::new(&root);
    let first = put_content(&store, chunks_of(b"abc".to_vec(), 1), "tmp-a")
        .await
        .expect("содержимое пишется");
    assert_eq!(
        first.key, "ba7816bf8f01cfea414140de5dae2223b00361a396177a9cb410ff61f20015ad",
        "ключ это sha256 содержимого в hex"
    );
    assert_eq!((first.size, first.existed), (3, false));
    let again = put_content(&store, chunks_of(b"abc".to_vec(), 2), "tmp-b")
        .await
        .expect("повтор пишется");
    assert_eq!(
        (again.key.as_str(), again.existed),
        (first.key.as_str(), true)
    );
    assert!(!store.exists("tmp-a").await.expect("проверка"));
    assert!(!store.exists("tmp-b").await.expect("проверка"));
    assert_eq!(hex(&[0, 15, 255]), "000fff");
}

#[tokio::test]
async fn limit_cuts_on_the_fly() {
    let early = limit(chunks_of(vec![0; 10], 4), 5, Some(10));
    assert_eq!(early.err(), Some(FileError::TooLarge { max: 5 }));
    let mut body = limit(chunks_of(vec![0; 10], 4), 5, None).expect("размер не заявлен");
    let first = body.next().await.expect("первый кусок");
    assert_eq!(first.expect("4 байта проходят").len(), 4);
    let second = body.next().await.expect("второй кусок");
    let error = second.expect_err("8 байт больше 5");
    assert_eq!(error.kind(), io::ErrorKind::InvalidData);
    assert!(body.next().await.is_none(), "после ошибки поток кончился");
    let mut fits = limit(chunks_of(vec![0; 5], 2), 5, Some(5)).expect("ровно предел");
    let mut total = 0;
    while let Some(piece) = fits.next().await {
        total += piece.expect("кусок в пределе").len();
    }
    assert_eq!(total, 5);
}

#[test]
fn type_comes_from_signature() {
    assert_eq!(sniff(&png(1, 1)), Some(Kind::Png));
    assert_eq!(sniff(&jpeg(1, 1)), Some(Kind::Jpeg));
    assert_eq!(sniff(&webp(1, 1)), Some(Kind::Webp));
    assert_eq!(sniff(b"<svg xmlns=\"http://www.w3.org/2000/svg\">"), None);
    assert_eq!(sniff(b"GIF89a\x01\0\x01\0"), None);
    assert_eq!(Kind::parse("image/jpeg"), Some(Kind::Jpeg));
    assert_eq!(
        (Kind::Webp.mime(), Kind::Webp.ext()),
        ("image/webp", "webp")
    );
}

#[test]
fn sizes_come_from_headers() {
    let checked = |data: Vec<u8>| check_avatar(&data).map(|p| (p.kind, p.width, p.height));
    assert_eq!(checked(png(640, 480)), Ok((Kind::Png, 640, 480)));
    assert_eq!(checked(jpeg(320, 200)), Ok((Kind::Jpeg, 320, 200)));
    assert_eq!(checked(webp(100, 50)), Ok((Kind::Webp, 100, 50)));
    assert_eq!(
        checked(png(MAX_SIDE + 1, 1)),
        Err(ImageError::BadSize {
            width: MAX_SIDE + 1,
            height: 1
        })
    );
    assert_eq!(
        checked(png(4000, 4001)),
        Err(ImageError::BadSize {
            width: 4000,
            height: 4001
        }),
        "площадь больше 16 млн пикселей"
    );
    assert!(matches!(
        checked(png(0, 10)),
        Err(ImageError::BadSize { .. })
    ));
    assert_eq!(
        checked(png(10, 10)[..14].to_vec()),
        Err(ImageError::Truncated)
    );
    assert_eq!(
        checked(jpeg(10, 10)[..24].to_vec()),
        Err(ImageError::Truncated)
    );
    assert_eq!(
        checked(b"plain text, nothing more".to_vec()),
        Err(ImageError::NotAnImage)
    );
}

#[test]
fn hmac_matches_rfc_4231() {
    assert_eq!(
        hex(&hmac_sha256(b"Jefe", b"what do ya want for nothing?")),
        "5bdcc146bf60754e6a042426089575c75a003f089d2739839dec58b964ec3843"
    );
    let long_key = [0xaa; 131];
    assert_eq!(
        hex(&hmac_sha256(
            &long_key,
            b"Test Using Larger Than Block-Size Key - Hash Key First"
        )),
        "60e431591ee0b67f0d8a26aacbf5b77f8e0bc6213728c5140546040f0ee37f54"
    );
}

#[test]
fn signed_links_expire_and_cannot_be_forged() {
    let signer = Signer::new(b"a key of sixteen+ bytes");
    let link = signer.link("abc123", 1_000, "avatar.png");
    assert!(
        link.starts_with("/files/abc123?exp=1000&name=avatar.png&sig="),
        "{link}"
    );
    let linked = signer.verify(&link, 999).expect("ссылка в сроке");
    assert_eq!(
        (linked.key.as_str(), linked.name.as_str(), linked.exp),
        ("abc123", "avatar.png", 1_000)
    );
    assert_eq!(signer.verify(&link, 1_000).err(), Some(LinkError::Expired));
    let longer = link.replace("exp=1000", "exp=9000");
    assert_eq!(
        signer.verify(&longer, 999).err(),
        Some(LinkError::Signature)
    );
    let other = link.replace("/files/abc123", "/files/abc124");
    assert_eq!(signer.verify(&other, 999).err(), Some(LinkError::Signature));
    let stranger = Signer::new(b"another key, also long").link("abc123", 1_000, "avatar.png");
    assert_eq!(
        signer.verify(&stranger, 999).err(),
        Some(LinkError::Signature)
    );
    for broken in [
        format!("{link}&x=1"),
        format!("{link}&exp=1000"),
        String::from("/files/abc123"),
        link.replace("name=avatar.png", "name=..%2Fpasswd"),
        link.replace("/files/abc123", "/files/../etc"),
    ] {
        assert_eq!(
            signer.verify(&broken, 999).err(),
            Some(LinkError::Format),
            "{broken}"
        );
    }
}

#[test]
fn served_files_never_sniff() {
    let header = |headers: &[(&str, String)], name: &str| {
        headers
            .iter()
            .find(|(key, _)| *key == name)
            .map(|(_, value)| value.clone())
    };
    let (how, headers) = safe_headers(&png(1, 1), "avatar.png");
    assert_eq!(how, Disposition::Inline);
    assert_eq!(
        header(&headers, "Content-Type").as_deref(),
        Some("image/png")
    );
    assert_eq!(
        header(&headers, "X-Content-Type-Options").as_deref(),
        Some("nosniff")
    );
    let (how, headers) = safe_headers(b"<script>alert(1)</script>", "evil\".html");
    assert_eq!(
        how,
        Disposition::Attachment,
        "незнакомое отдаётся вложением"
    );
    assert_eq!(
        header(&headers, "Content-Type").as_deref(),
        Some("application/octet-stream")
    );
    let disposition = header(&headers, "Content-Disposition").expect("есть Content-Disposition");
    assert!(
        disposition.starts_with("attachment; filename=\"evil.html\""),
        "{disposition}"
    );
    assert!(header(&headers, "Content-Security-Policy").is_some_and(|csp| csp.contains("sandbox")));
}
