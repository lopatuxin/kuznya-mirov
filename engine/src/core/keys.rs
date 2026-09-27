use std::collections::HashMap;

use super::property::PropertyId;
use super::value::Value;

/// «Мышь в мире», требование 17–18: одно значение записи привязки (`keys`' `press`/`release`
/// или `on_click`) — либо обычная константа, либо `"cursor"`, точка под курсором в момент, когда
/// пришло это нажатие или отпускание. Проверено при загрузке: `Cursor` встречается только у
/// свойства вида `Vec2`.
#[derive(Debug, Clone, PartialEq)]
pub enum EditValue {
    Const(Value),
    Cursor,
}

#[derive(Debug, Clone, PartialEq)]
pub struct KeyEdit {
    pub property: PropertyId,
    pub value: EditValue,
}

#[derive(Debug, Clone, Default, PartialEq)]
pub struct KeyBinding {
    pub press: Vec<KeyEdit>,
    pub release: Vec<KeyEdit>,
}

pub type KeyTable = HashMap<String, KeyBinding>;
