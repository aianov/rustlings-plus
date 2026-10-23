pub struct Line {
    pub name: String,
    pub cents: u64,
    pub qty: u32,
}

pub struct Invoice {
    lines: Vec<Line>,
    discount_percent: u8,
}

impl Invoice {
    pub fn new() -> Invoice {
        Invoice { lines: Vec::new(), discount_percent: 0 }
    }

    pub fn add(&mut self, name: &str, cents: u64, qty: u32) -> &mut Invoice {
        self.lines.push(Line { name: name.to_string(), cents, qty });
        self
    }

    pub fn discount(&mut self, percent: u8) -> &mut Invoice {
        self.discount_percent = percent.min(100);
        self
    }

    pub fn line_count(&self) -> usize {
        self.lines.len()
    }

    pub fn subtotal(&self) -> u64 {
        self.lines.iter().map(|line| line.cents * u64::from(line.qty)).sum()
    }

    pub fn total(&self) -> u64 {
        self.subtotal() * (100 - u64::from(self.discount_percent)) / 100
    }
}

impl Default for Invoice {
    fn default() -> Self {
        Invoice::new()
    }
}
