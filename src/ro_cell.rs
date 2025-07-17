use std::{cell::UnsafeCell, mem::MaybeUninit, ops::Deref};

pub struct RoCell<T> {
    content: UnsafeCell<MaybeUninit<T>>,
    initialized: UnsafeCell<bool>,
}

unsafe impl<T> Sync for RoCell<T> {}

impl<T> RoCell<T> {
    pub const fn new() -> Self {
        Self {
            content: UnsafeCell::new(MaybeUninit::uninit()),
            initialized: UnsafeCell::new(false),
        }
    }

    pub fn init(&self, value: T) {
        unsafe {
            self.initialized.get().replace(true);
            *self.content.get() = MaybeUninit::new(value);
        }
    }
}

impl<T> Deref for RoCell<T> {
    type Target = T;

    fn deref(&self) -> &Self::Target {
        unsafe { (*self.content.get()).assume_init_ref() }
    }
}
