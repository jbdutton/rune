use crate::no_std::prelude::*;

use crate::runtime::{Stack, ToValue, Value, VmResult};

/// Trait for converting arguments onto the stack.
pub trait Args {
    /// Encode arguments onto a stack.
    fn into_stack(self, stack: &mut Stack) -> VmResult<()>;

    /// Convert arguments into a vector.
    fn into_vec(self) -> VmResult<Vec<Value>>;

    /// The number of arguments.
    fn count(&self) -> usize;
}

macro_rules! impl_into_args {
    ($count:expr $(, $ty:ident $value:ident $_:expr)*) => {
        impl<$($ty,)*> Args for ($($ty,)*)
        where
            $($ty: ToValue,)*
        {
            #[allow(unused)]
            fn into_stack(self, stack: &mut Stack) -> VmResult<()> {
                let ($($value,)*) = self;
                $(stack.push(vm_try!($value.to_value()));)*
                VmResult::Ok(())
            }

            #[allow(unused)]
            fn into_vec(self) -> VmResult<Vec<Value>> {
                let ($($value,)*) = self;
                $(let $value = vm_try!(<$ty>::to_value($value));)*
                VmResult::Ok(vec![$($value,)*])
            }

            fn count(&self) -> usize {
                $count
            }
        }
    };
}

repeat_macro!(impl_into_args);

impl Args for Vec<Value> {
    fn into_stack(self, stack: &mut Stack) -> VmResult<()> {
        for value in self {
            stack.push(value);
        }

        VmResult::Ok(())
    }

    fn into_vec(self) -> VmResult<Vec<Value>> {
        VmResult::Ok(self)
    }

    fn count(&self) -> usize {
        self.len()
    }
}
