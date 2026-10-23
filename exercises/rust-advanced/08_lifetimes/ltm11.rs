// Корзина считает, сколько раз её правили (`edits`), и дарит метод `items_mut` для внешних правок. Скидка и
// чистка пустых позиций пишутся через него же, и обе функции не собираются: «cannot use `self.edits`
// because it was mutably borrowed» (E0503) и «closure requires unique access» (E0500). Метод `items_mut()` берёт взаймы всю корзину целиком,
// хотя в теле нужен один вектор, и счётчик рядом оказывается недоступен.
// TODO: Почини `apply_discount` и `remove_empty`, не удаляя `items_mut`: он нужен снаружи.

pub struct Item {
    pub name: String,
    pub qty: u32,
    pub price: u32,
}

pub struct Cart {
    items: Vec<Item>,
    edits: u32,
}

impl Cart {
    pub fn new(items: Vec<Item>) -> Cart {
        Cart { items, edits: 0 }
    }

    pub fn items_mut(&mut self) -> &mut Vec<Item> {
        &mut self.items
    }

    pub fn items(&self) -> &[Item] {
        &self.items
    }

    pub fn edits(&self) -> u32 {
        self.edits
    }

    pub fn apply_discount(&mut self, percent: u32) {
        for item in self.items_mut().iter_mut() {
            item.price = item.price * (100 - percent) / 100;
            self.edits += 1;
        }
    }

    pub fn remove_empty(&mut self) {
        self.items_mut().retain(|item| {
            let keep = item.qty > 0;
            if !keep {
                self.edits += 1;
            }
            keep
        });
    }
}

fn main() {
    let mut cart = Cart::new(vec![Item { name: "pen".into(), qty: 1, price: 200 }]);
    cart.apply_discount(10);
    println!("{}", cart.items()[0].price);
}

#[cfg(test)]
mod tests {
    use super::*;

    fn cart() -> Cart {
        Cart::new(vec![
            Item { name: "pen".into(), qty: 2, price: 200 },
            Item { name: "ink".into(), qty: 0, price: 500 },
            Item { name: "pad".into(), qty: 1, price: 1000 },
        ])
    }

    #[test]
    fn discount_changes_prices_and_counts_edits() {
        let mut c = cart();
        c.apply_discount(10);
        let prices: Vec<u32> = c.items().iter().map(|i| i.price).collect();
        assert_eq!(prices, vec![180, 450, 900]);
        assert_eq!(c.edits(), 3, "каждая правка цены считается");
    }

    #[test]
    fn empty_positions_are_removed_and_counted() {
        let mut c = cart();
        c.remove_empty();
        let names: Vec<&str> = c.items().iter().map(|i| i.name.as_str()).collect();
        assert_eq!(names, vec!["pen", "pad"]);
        assert_eq!(c.edits(), 1, "считается только удалённая позиция");
    }

    #[test]
    fn items_mut_is_still_public() {
        let mut c = cart();
        c.items_mut().push(Item { name: "cap".into(), qty: 1, price: 10 });
        assert_eq!(c.items().len(), 4);
        assert_eq!(c.edits(), 0, "внешние правки счётчик не трогают");
    }
}
