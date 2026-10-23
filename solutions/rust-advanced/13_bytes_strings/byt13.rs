pub const FIN: u8 = 0b1000_0000;
pub const RESERVED: u8 = 0b0111_0000;
pub const OPCODE: u8 = 0b0000_1111;

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct FrameByte(pub u8);

impl FrameByte {
    pub fn new(fin: bool, opcode: u8) -> Option<FrameByte> {
        FrameByte(0).with_fin(fin).with_opcode(opcode)
    }

    pub fn fin(self) -> bool {
        self.0 & FIN != 0
    }

    pub fn opcode(self) -> u8 {
        self.0 & OPCODE
    }

    pub fn reserved(self) -> u8 {
        (self.0 & RESERVED) >> 4
    }

    pub fn with_fin(self, fin: bool) -> FrameByte {
        if fin { FrameByte(self.0 | FIN) } else { FrameByte(self.0 & !FIN) }
    }

    pub fn with_opcode(self, opcode: u8) -> Option<FrameByte> {
        if opcode > OPCODE {
            return None;
        }
        Some(FrameByte((self.0 & !OPCODE) | opcode))
    }
}

fn main() {
    println!("{:?}", FrameByte::new(true, 1));
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_byte_is_assembled_from_fields() {
        assert_eq!(FrameByte::new(true, 1), Some(FrameByte(0b1000_0001)));
        assert_eq!(FrameByte::new(false, 10), Some(FrameByte(0b0000_1010)));
        assert_eq!(FrameByte::new(true, 15), Some(FrameByte(0b1000_1111)));
        assert_eq!(FrameByte::new(true, 16), None, "код операции длиннее четырёх бит");
    }

    #[test]
    fn fields_are_read_independently() {
        let b = FrameByte(0b1101_0011);
        assert!(b.fin());
        assert_eq!(b.reserved(), 0b101);
        assert_eq!(b.opcode(), 0b0011);
        let c = FrameByte(0b0010_1000);
        assert!(!c.fin());
        assert_eq!(c.reserved(), 0b010);
        assert_eq!(c.opcode(), 8);
    }

    #[test]
    fn changing_fin_keeps_other_bits() {
        let b = FrameByte(0b0101_0110);
        assert_eq!(b.with_fin(true), FrameByte(0b1101_0110));
        assert_eq!(FrameByte(0b1101_0110).with_fin(false), b);
        assert_eq!(b.with_fin(false), b, "снять уже снятый бит: ничего не меняется");
    }

    #[test]
    fn changing_the_opcode_keeps_other_bits() {
        let b = FrameByte(0b1101_0110);
        assert_eq!(b.with_opcode(0b0001), Some(FrameByte(0b1101_0001)));
        assert_eq!(b.with_opcode(0), Some(FrameByte(0b1101_0000)), "старый код должен сначала стереться");
        assert_eq!(b.with_opcode(16), None);
        assert_eq!(b.with_opcode(255), None);
    }
}
