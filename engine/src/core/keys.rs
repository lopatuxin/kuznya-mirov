use std::collections::HashMap;

use super::property::PropertyId;
use super::value::Value;

#[derive(Debug, Clone, PartialEq)]
pub struct KeyEdit {
    pub property: PropertyId,
    pub value: Value,
}

#[derive(Debug, Clone, Default, PartialEq)]
pub struct KeyBinding {
    pub press: Vec<KeyEdit>,
    pub release: Vec<KeyEdit>,
}

pub type KeyTable = HashMap<String, KeyBinding>;
