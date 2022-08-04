#![cfg(test)]

use std::collections::HashMap;
use std::pin::Pin;

use fxhash::FxHashMap;
use proptest::prelude::*;
use proptest::proptest;

use crate::value::proptest::gen_insert_value;
use crate::InternalString;
use crate::{
    configure::Configure,
    container::{Container, ContainerType},
    fx_map,
    value::InsertValue,
    LoroCore, LoroValue,
};

use super::*;

#[test]
fn basic() {
    let mut loro = LoroCore::default();
    let mut container = loro.get_map_container("map".into());
    container.insert("haha".into(), InsertValue::Int32(1));
    let ans = fx_map!(
        "haha".into() => LoroValue::Integer(1)
    );

    assert_eq!(*container.get_value(), LoroValue::Map(ans));
}

#[cfg(not(no_proptest))]
mod map_proptest {
    use super::*;

    proptest! {
        #[test]
        fn insert(
            key in prop::collection::vec("[a-z]", 0..100),
            value in prop::collection::vec(gen_insert_value(), 0..100)
        ) {
            let mut loro = LoroCore::default();
            let mut container = loro.get_map_container("map".into());
            let mut map: HashMap<String, InsertValue> = HashMap::new();
            for (k, v) in key.iter().zip(value.iter()) {
                map.insert(k.clone(), v.clone());
                container.insert(k.clone().into(), v.clone());
                let snapshot = container.get_value();
                for (key, value) in snapshot.as_map().unwrap().iter() {
                    assert_eq!(map.get(&key.to_string()).map(|x|x.clone().into()), Some(value.clone()));
                }
            }
        }
    }
}