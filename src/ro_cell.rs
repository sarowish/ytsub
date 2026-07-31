use std::{cell::UnsafeCell, mem::MaybeUninit, ops::Deref};

pub struct RoCell<T> {
    content: UnsafeCell<MaybeUninit<T>>,
    #[cfg(debug_assertions)]
    initialized: UnsafeCell<bool>,
}

unsafe impl<T> Sync for RoCell<T> {}

impl<T> RoCell<T> {
    pub const fn new() -> Self {
        Self {
            content: UnsafeCell::new(MaybeUninit::uninit()),
            #[cfg(debug_assertions)]
            initialized: UnsafeCell::new(false),
        }
    }

    pub fn init(&self, value: T) {
        unsafe {
            #[cfg(debug_assertions)]
            assert!(!self.initialized.get().replace(true));
            *self.content.get() = MaybeUninit::new(value);
        }
    }
}

impl<T> Deref for RoCell<T> {
    type Target = T;

    fn deref(&self) -> &Self::Target {
        unsafe {
            #[cfg(debug_assertions)]
            assert!(*self.initialized.get());
            (*self.content.get()).assume_init_ref()
        }
    }
}
