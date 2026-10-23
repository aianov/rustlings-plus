// Владелец и его устройства: владелец знает свои устройства, а устройство должно уметь назвать владельца.
// Обе ссылки сделаны через `Rc`, и получилась петля: владелец держит устройства, устройства держат владельца,
// и когда все внешние ссылки пропали, счётчики не доходят до нуля. Память утекает, а `Drop` не вызывается
// вообще (тест считает уничтоженные объекты). Утечка не ошибка в смысле безопасности Rust, поэтому
// компилятор молчит. Разорви петлю там, где ссылка идёт «вверх», по принципу «кто кому принадлежит».
// TODO: Почини `Gadget`, чтобы после падения последних внешних ссылок уничтожились и владелец, и устройства.
use std::cell::{Cell, RefCell};
use std::rc::Rc;

pub type Drops = Rc<Cell<u32>>;

pub struct Owner {
    name: String,
    gadgets: RefCell<Vec<Rc<Gadget>>>,
    drops: Drops,
}

pub struct Gadget {
    pub id: u32,
    owner: Rc<Owner>,
    drops: Drops,
}

impl Drop for Owner {
    fn drop(&mut self) {
        self.drops.set(self.drops.get() + 1);
    }
}

impl Drop for Gadget {
    fn drop(&mut self) {
        self.drops.set(self.drops.get() + 1);
    }
}

impl Owner {
    pub fn new(name: &str, drops: &Drops) -> Rc<Owner> {
        Rc::new(Owner { name: name.to_string(), gadgets: RefCell::new(Vec::new()), drops: Rc::clone(drops) })
    }

    pub fn gadget_count(&self) -> usize {
        self.gadgets.borrow().len()
    }
}

impl Gadget {
    pub fn attach(owner: &Rc<Owner>, id: u32) -> Rc<Gadget> {
        let gadget = Rc::new(Gadget { id, owner: Rc::clone(owner), drops: Rc::clone(&owner.drops) });
        owner.gadgets.borrow_mut().push(Rc::clone(&gadget));
        gadget
    }

    pub fn owner_name(&self) -> Option<String> {
        Some(self.owner.name.clone())
    }
}

fn main() {
    let drops: Drops = Rc::default();
    let owner = Owner::new("ann", &drops);
    let phone = Gadget::attach(&owner, 1);
    println!("{:?}", phone.owner_name());
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn gadgets_know_their_owner() {
        let drops: Drops = Rc::default();
        let owner = Owner::new("ann", &drops);
        let phone = Gadget::attach(&owner, 1);
        let watch = Gadget::attach(&owner, 2);
        assert_eq!(phone.owner_name().as_deref(), Some("ann"));
        assert_eq!(watch.id, 2);
        assert_eq!(owner.gadget_count(), 2);
    }

    #[test]
    fn everything_is_freed_when_the_last_outside_handle_goes() {
        let drops: Drops = Rc::default();
        {
            let owner = Owner::new("ann", &drops);
            let _phone = Gadget::attach(&owner, 1);
            let _watch = Gadget::attach(&owner, 2);
            assert_eq!(drops.get(), 0);
        }
        assert_eq!(drops.get(), 3, "владелец и два устройства должны быть уничтожены: если Drop не вызван, где-то остался цикл из Rc");
    }

    #[test]
    fn a_gadget_can_outlive_its_owner() {
        let drops: Drops = Rc::default();
        let owner = Owner::new("bob", &drops);
        let phone = Gadget::attach(&owner, 1);
        drop(owner);
        assert_eq!(drops.get(), 1, "владелец уничтожен, потому что устройство его не держит");
        assert_eq!(phone.owner_name(), None, "у устройства больше нет владельца");
        drop(phone);
        assert_eq!(drops.get(), 2);
    }
}
