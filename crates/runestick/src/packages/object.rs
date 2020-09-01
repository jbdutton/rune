//! Package containing object functions.

use crate::{ContextError, Module, Object, Value};
use std::iter::Rev;

/// An iterator over a vector.
pub struct Iter {
    iter: std::vec::IntoIter<(String, Value)>,
}

impl Iterator for Iter {
    type Item = (String, Value);

    fn next(&mut self) -> Option<Self::Item> {
        self.iter.next()
    }
}

impl DoubleEndedIterator for Iter {
    fn next_back(&mut self) -> Option<Self::Item> {
        self.iter.next_back()
    }
}

fn object_iter(object: &Object<Value>) -> Iter {
    Iter {
        iter: object
            .iter()
            .map(|(k, v)| (k.clone(), v.clone()))
            .collect::<Vec<_>>()
            .into_iter(),
    }
}

fn contains_key(object: &Object<Value>, key: &str) -> bool {
    object.contains_key(key)
}

fn get(object: &Object<Value>, key: &str) -> Option<Value> {
    object.get(key).cloned()
}

decl_external!(Iter);
decl_external!(Rev<Iter>);

/// Get the module for the object package.
pub fn module() -> Result<Module, ContextError> {
    let mut module = Module::new(&["std", "object"]);

    module.ty(&["Object"]).build::<Object<Value>>()?;
    module.ty(&["Iter"]).build::<Iter>()?;
    module.ty(&["Rev"]).build::<Rev<Iter>>()?;

    module.inst_fn("len", Object::<Value>::len)?;
    module.inst_fn("insert", Object::<Value>::insert)?;
    module.inst_fn("clear", Object::<Value>::clear)?;
    module.inst_fn("contains_key", contains_key)?;
    module.inst_fn("get", get)?;

    module.inst_fn(crate::INTO_ITER, object_iter)?;
    module.inst_fn("next", Iter::next)?;
    module.inst_fn(crate::NEXT, Iter::next)?;
    module.inst_fn(crate::INTO_ITER, Iter::into_iter)?;

    module.inst_fn("rev", Iter::rev)?;
    module.inst_fn("next", Rev::<Iter>::next)?;
    module.inst_fn("next_back", Rev::<Iter>::next_back)?;
    module.inst_fn(crate::NEXT, Rev::<Iter>::next)?;
    module.inst_fn(crate::INTO_ITER, Rev::<Iter>::into_iter)?;

    Ok(module)
}
