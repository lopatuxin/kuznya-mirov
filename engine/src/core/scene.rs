#[derive(Debug, Clone, Copy)]
pub struct SceneConfig {
    pub width: u32,
    pub height: u32,
    pub background: [f32; 4],
}

/// One `scene.json` object, kept around after load so «Экраны и состояние»'s `new_game` can
/// rebuild the world from this parsed copy without reopening the file a second time.
#[derive(Debug, Clone)]
pub struct ObjectSpec {
    pub values: Vec<(super::property::PropertyId, super::value::Value)>,
    pub grid: Option<super::value::GridSpec>,
    pub keys: Option<super::keys::KeyTable>,
}
