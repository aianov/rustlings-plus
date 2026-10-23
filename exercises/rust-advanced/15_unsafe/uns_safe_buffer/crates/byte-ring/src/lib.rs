//! Кольцевой буфер байтов с unsafe внутри и безопасным API.
//!
//! TODO: Реализуй `ByteRing` по README. Подумай об инвариантах: что хранят `head` и `len`, какие
//! байты внутри буфера уже записаны, и почему арифметика индексов идёт по модулю ёмкости.

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Full;

pub struct ByteRing {}

impl ByteRing {
    pub fn with_capacity(capacity: usize) -> ByteRing {
        todo!("выделить память на capacity байт")
    }

    pub fn capacity(&self) -> usize {
        todo!("ёмкость")
    }

    pub fn len(&self) -> usize {
        todo!("сколько байт лежит")
    }

    pub fn is_empty(&self) -> bool {
        todo!("пуст ли")
    }

    pub fn is_full(&self) -> bool {
        todo!("полон ли")
    }

    pub fn push(&mut self, byte: u8) -> Result<(), Full> {
        todo!("положить байт в конец")
    }

    pub fn pop(&mut self) -> Option<u8> {
        todo!("забрать самый старый байт")
    }

    pub fn peek(&self) -> Option<u8> {
        todo!("посмотреть самый старый байт")
    }

    pub fn write(&mut self, data: &[u8]) -> usize {
        todo!("записать сколько влезет, вернуть число")
    }

    pub fn read(&mut self, out: &mut [u8]) -> usize {
        todo!("прочитать сколько есть, вернуть число")
    }

    pub fn as_slices(&self) -> (&[u8], &[u8]) {
        todo!("содержимое двумя срезами по порядку")
    }

    pub fn discard(&mut self, count: usize) -> usize {
        todo!("выбросить до count старых байт")
    }

    pub fn clear(&mut self) {
        todo!("опустошить")
    }
}
