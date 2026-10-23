// Слушатель между клиентом и сервером видит байты. Научись читать то, что он увидит:
// есть ли в записи пароль, похожа ли она на TLS и к какому имени идёт клиент (SNI).
#![allow(unused_variables)]

pub mod tap;

pub fn contains(hay: &[u8], needle: &[u8]) -> bool {
    todo!()
}

pub fn looks_like_tls(bytes: &[u8]) -> bool {
    todo!()
}


pub fn sni(bytes: &[u8]) -> Option<String> {
    todo!()
}
