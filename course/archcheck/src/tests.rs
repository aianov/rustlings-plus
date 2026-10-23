use super::*;
use std::path::PathBuf;

struct Dir(PathBuf);

impl Dir {
    fn new(tag: &str, files: &[(&str, &str)]) -> Dir {
        let p = std::env::temp_dir().join(format!("archcheck-test-{}-{tag}", std::process::id()));
        let _ = fs::remove_dir_all(&p);
        fs::create_dir_all(&p).unwrap();
        for (name, body) in files {
            let f = p.join(name);
            fs::create_dir_all(f.parent().unwrap()).unwrap();
            fs::write(f, body).unwrap();
        }
        Dir(p)
    }

    fn run(&self, rules: &str) -> Vec<Violation> {
        check(&self.0, rules, "en").unwrap()
    }
}

impl Drop for Dir {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.0);
    }
}

fn paths(v: &[Violation]) -> Vec<String> {
    v.iter().filter_map(|x| x.path.clone()).collect()
}

#[test]
fn exists_rule() {
    let d = Dir::new("exists", &[("Cargo.toml", ""), ("src/lib.rs", "")]);
    let v = d.run(
        "[[rules]]\nkind='exists'\npaths=['Cargo.toml','src','src/main.rs']\nmessage='need'\n",
    );
    assert_eq!(paths(&v), ["src/main.rs"]);
    assert_eq!(v[0].message, "need");
    assert_eq!(v[0].rule, "exists");
}

#[test]
fn absent_rule_and_skips() {
    let d = Dir::new(
        "absent",
        &[
            ("src/dto/a.rs", ""),
            ("src/utils.rs", ""),
            ("a/.gitkeep", ""),
            ("target/utils.rs", ""),
            (".hid/utils.rs", ""),
            ("src/ok.rs", ""),
        ],
    );
    let v = d.run("[[rules]]\nkind='absent'\nglobs=['**/dto/**','**/.gitkeep','**/utils.rs']\n");
    assert_eq!(
        paths(&v),
        ["a/.gitkeep", "src/dto", "src/dto/a.rs", "src/utils.rs"]
    );
}

#[test]
fn uniform_modules_rule() {
    let bad = Dir::new(
        "um-bad",
        &[
            ("src/lib.rs", ""),
            ("src/x.rs", ""),
            ("src/m/mod.rs", ""),
            ("src/m/y.rs", ""),
        ],
    );
    let rules = "[[rules]]\nkind='uniform_modules'\nroots=['src']\n";
    assert_eq!(paths(&bad.run(rules)), ["src/x.rs"]);
    let flat = Dir::new(
        "um-flat",
        &[
            ("src/lib.rs", ""),
            ("src/x.rs", ""),
            ("src/data/a.json", ""),
        ],
    );
    assert!(flat.run(rules).is_empty());
    let folders = Dir::new(
        "um-dirs",
        &[
            ("src/main.rs", ""),
            ("src/a/mod.rs", ""),
            ("src/b/mod.rs", ""),
            ("src/b/deep/q.rs", ""),
            ("src/b/deep/r/mod.rs", ""),
        ],
    );
    assert_eq!(paths(&folders.run(rules)), ["src/b/deep/q.rs"]);
}

#[test]
fn no_comments_rule() {
    let d = Dir::new(
        "nc",
        &[
            ("src/a.rs", "fn a() {}\n// c\n"),
            ("src/b.rs", "let s = \"//\";\n"),
            ("src/c.rs", "/* x */"),
            ("src/d.txt", "// x"),
        ],
    );
    let v = d.run("[[rules]]\nkind='no_comments'\nglobs=['src/**/*']\n");
    assert_eq!(paths(&v), ["src/a.rs", "src/c.rs"]);
    assert!(v[0].message.contains("line 2"));
}

#[test]
fn workspace_members_rule() {
    let rules = "[[rules]]\nkind='workspace_members'\nmembers=['crates/a','crates/b','svc/*']\n";
    let ok = Dir::new(
        "wm-ok",
        &[
            ("Cargo.toml", "[workspace]\nmembers=['crates/*','svc/x']\n"),
            ("crates/a/Cargo.toml", ""),
            ("crates/b/Cargo.toml", ""),
            ("svc/x/Cargo.toml", ""),
        ],
    );
    assert!(ok.run(rules).is_empty());
    let bad = Dir::new(
        "wm-bad",
        &[
            ("Cargo.toml", "[workspace]\nmembers=['crates/a']\n"),
            ("crates/a/x", ""),
            ("crates/b/x", ""),
            ("svc/x/x", ""),
        ],
    );
    assert_eq!(paths(&bad.run(rules)), ["crates/b", "svc/x"]);
    let none = Dir::new("wm-none", &[("Cargo.toml", "[package]\nname='a'\n")]);
    assert_eq!(paths(&none.run(rules)), ["Cargo.toml"]);
}

#[test]
fn workspace_deps_rule() {
    let d = Dir::new(
        "wd",
        &[
            (
                "crates/a/Cargo.toml",
                "[dependencies]\nserde.workspace = true\ntokio = { workspace = true }\n",
            ),
            (
                "crates/b/Cargo.toml",
                "[dependencies]\nserde = '1'\n[dev-dependencies]\ntokio = { version = '1' }\n",
            ),
        ],
    );
    let v = d.run("[[rules]]\nkind='workspace_deps'\npackages=['crates/*']\n");
    assert_eq!(paths(&v), ["crates/b/Cargo.toml", "crates/b/Cargo.toml"]);
}

#[test]
fn deps_rule() {
    let d = Dir::new(
        "deps",
        &[(
            "p/Cargo.toml",
            "[dependencies]\ntokio='1'\nhyper='1'\nweb = { package = 'axum', version = '1' }\n[dev-dependencies]\nserde='1'\n",
        )],
    );
    let v = d.run("[[rules]]\nkind='deps'\npackage='p'\nforbid=['hyper','axum']\n");
    assert_eq!(v.len(), 2);
    let v = d.run("[[rules]]\nkind='deps'\npackage='p'\nallow_only=['tokio','serde']\n");
    assert_eq!(v.len(), 2);
    assert!(d.run("[[rules]]\nkind='deps'\npackage='p'\nforbid=['x']\nallow_only=['tokio','hyper','web','serde']\n").is_empty());
}

#[test]
fn contains_rules() {
    let d = Dir::new(
        "ct",
        &[
            ("s/a/main.rs", "let l = TcpListener::bind(x);"),
            ("s/b/main.rs", "fn main() { x.unwrap() }"),
        ],
    );
    let v = d.run("[[rules]]\nkind='contains'\nglobs=['s/*/main.rs']\nregex='TcpListener::bind'\n");
    assert_eq!(paths(&v), ["s/b/main.rs"]);
    let v = d.run("[[rules]]\nkind='contains'\nglobs=['none/*.rs']\nregex='x'\n");
    assert_eq!(v.len(), 1);
    assert_eq!(v[0].path, None);
    let v = d.run("[[rules]]\nkind='not_contains'\nglobs=['s/**/*.rs']\nregex='\\.unwrap\\(\\)'\n");
    assert_eq!(paths(&v), ["s/b/main.rs"]);
    assert!(
        d.run("[[rules]]\nkind='not_contains'\nglobs=['none/*.rs']\nregex='x'\n")
            .is_empty()
    );
}

#[test]
fn max_lines_rule() {
    let d = Dir::new("ml", &[("a.rs", "1\n2\n3\n"), ("b.rs", "1\n2\n3\n4\n")]);
    let v = d.run("[[rules]]\nkind='max_lines'\nglobs=['*.rs']\nmax=3\n");
    assert_eq!(paths(&v), ["b.rs"]);
}

#[test]
fn allowed_children_rule() {
    let d = Dir::new(
        "ac",
        &[
            ("m/x/src/main.rs", ""),
            ("m/x/src/api/a.rs", ""),
            ("m/x/src/junk.rs", ""),
            ("m/y/src/lib.rs", ""),
        ],
    );
    let v = d.run("[[rules]]\nkind='allowed_children'\ndirs=['m/*/src']\nallowed=['main.rs','lib.rs','api']\n");
    assert_eq!(paths(&v), ["m/x/src/junk.rs"]);
}

#[test]
fn locale_messages() {
    let d = Dir::new("loc", &[]);
    let rules =
        "[[rules]]\nkind='exists'\npaths=['a']\nmessage.en='english'\nmessage.ru='русский'\n";
    assert_eq!(check(&d.0, rules, "ru").unwrap()[0].message, "русский");
    assert_eq!(check(&d.0, rules, "en").unwrap()[0].message, "english");
    assert_eq!(check(&d.0, rules, "de").unwrap()[0].message, "english");
}

#[test]
fn errors() {
    let d = Dir::new("err", &[]);
    assert!(check(&d.0, "[[rules]]\nkind='nope'\n", "en").is_err());
    assert!(check(&d.0, "[[rules]]\nkind='exists'\n", "en").is_err());
    assert!(check(&d.0, "[[rules]]\nkind='exists'\npaths=[]\nbogus=1\n", "en").is_err());
    assert!(check(&d.0.join("missing"), "", "en").is_err());
    assert!(
        check(
            &d.0,
            "[[rules]]\nkind='contains'\nglobs=['*']\nregex='*'\n",
            "en"
        )
        .is_err()
    );
}
