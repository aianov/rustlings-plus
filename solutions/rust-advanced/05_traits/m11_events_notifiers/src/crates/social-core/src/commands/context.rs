use std::any::{Any, TypeId, type_name};
use std::collections::HashMap;

use super::CommandError;

#[derive(Default)]
pub struct Context {
    extensions: HashMap<TypeId, Box<dyn Any + Send + Sync>>,
}

impl Context {
    pub fn new() -> Context {
        Context::default()
    }

    pub fn insert<T: Any + Send + Sync>(&mut self, value: T) -> Option<T> {
        let old = self.extensions.insert(TypeId::of::<T>(), Box::new(value))?;
        let old: Box<dyn Any> = old;
        old.downcast::<T>().ok().map(|old| *old)
    }

    pub fn get<T: Any + Send + Sync>(&self) -> Option<&T> {
        self.extensions.get(&TypeId::of::<T>())?.downcast_ref::<T>()
    }

    pub fn get_mut<T: Any + Send + Sync>(&mut self) -> Option<&mut T> {
        self.extensions
            .get_mut(&TypeId::of::<T>())?
            .downcast_mut::<T>()
    }

    pub fn require<T: Any + Send + Sync>(&self) -> Result<&T, CommandError> {
        self.get::<T>()
            .ok_or_else(|| CommandError::NoExtension(type_name::<T>()))
    }
}
