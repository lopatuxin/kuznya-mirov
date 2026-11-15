//! «Экраны и состояние» → «Настройки игрока»: выбор игрока, который живёт на экранах, а не в мире, —
//! громкость двух шин и замены клавиш.

use std::collections::BTreeMap;

use serde_json::{Value as Json, json};

pub const MAX_VOLUME: u8 = 100;

/// Громкость шины: `setting` ползунка и подстановки `{setting:…}`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Volume {
    Music,
    Sound,
}

impl Volume {
    pub fn parse(name: &str) -> Option<Volume> {
        match name {
            "music_volume" => Some(Volume::Music),
            "sound_volume" => Some(Volume::Sound),
            _ => None,
        }
    }
}

/// `setting` переключателя.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Toggle {
    Sound,
    Fullscreen,
}

impl Toggle {
    pub fn parse(name: &str) -> Option<Toggle> {
        match name {
            "sound" => Some(Toggle::Sound),
            "fullscreen" => Some(Toggle::Fullscreen),
            _ => None,
        }
    }
}

/// «клавиша игры → клавиша игрока», только отличные от самой клавиши игры. Состояние всегда
/// перестановка: назначение занятой клавиши меняет две клавиши игры местами.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct KeyReplacements {
    by_game_key: BTreeMap<String, String>,
}

impl KeyReplacements {
    /// Клавиша игрока, которая сейчас стоит на клавише игры.
    pub fn player_key<'a>(&'a self, game_key: &'a str) -> &'a str {
        self.by_game_key
            .get(game_key)
            .map_or(game_key, String::as_str)
    }

    /// Какой клавишей игры мир видит нажатую клавишу `code`; `None` — клавишу у игры отняла замена.
    pub fn world_key<'a>(&'a self, code: &'a str) -> Option<&'a str> {
        if let Some((game_key, _)) = self
            .by_game_key
            .iter()
            .find(|(_, player_key)| player_key.as_str() == code)
        {
            return Some(game_key);
        }
        (!self.by_game_key.contains_key(code)).then_some(code)
    }

    /// Ставит `player_key` на `game_key`; клавиша игры, на которой он уже стоял, получает прежнюю клавишу
    /// первой. `game_keys` — все клавиши игры.
    pub fn assign(&mut self, game_keys: &[String], game_key: &str, player_key: &str) {
        let previous = self.player_key(game_key).to_string();
        if previous == player_key {
            return;
        }
        let occupant = game_keys
            .iter()
            .find(|other| other.as_str() != game_key && self.player_key(other) == player_key)
            .cloned();
        if let Some(other) = occupant {
            self.set(&other, previous);
        }
        self.set(game_key, player_key.to_string());
    }

    fn set(&mut self, game_key: &str, player_key: String) {
        if player_key == game_key {
            self.by_game_key.remove(game_key);
        } else {
            self.by_game_key.insert(game_key.to_string(), player_key);
        }
    }

    /// Убирает все замены; `true`, если они были.
    pub fn clear(&mut self) -> bool {
        let had = !self.by_game_key.is_empty();
        self.by_game_key.clear();
        had
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PlayerSettings {
    music_volume: u8,
    sound_volume: u8,
    pub keys: KeyReplacements,
}

impl Default for PlayerSettings {
    fn default() -> Self {
        PlayerSettings {
            music_volume: MAX_VOLUME,
            sound_volume: MAX_VOLUME,
            keys: KeyReplacements::default(),
        }
    }
}

impl PlayerSettings {
    pub fn volume(&self, volume: Volume) -> u8 {
        match volume {
            Volume::Music => self.music_volume,
            Volume::Sound => self.sound_volume,
        }
    }

    /// `true`, если громкость изменилась.
    pub fn set_volume(&mut self, volume: Volume, value: u8) -> bool {
        let value = value.min(MAX_VOLUME);
        let slot = match volume {
            Volume::Music => &mut self.music_volume,
            Volume::Sound => &mut self.sound_volume,
        };
        let changed = *slot != value;
        *slot = value;
        changed
    }

    pub fn to_json(&self) -> String {
        json!({
            "music_volume": self.music_volume,
            "sound_volume": self.sound_volume,
            "keys": self.keys.by_game_key,
        })
        .to_string()
    }

    /// Принимает сохранённые настройки; что игре не подходит — не JSON, число не целое или вне 0–100, клавиши
    /// игры нет среди `game_keys`, клавиша игрока занята экраном (`reserved`) — молча отбрасывает.
    pub fn load_json(&mut self, text: &str, game_keys: &[String], reserved: impl Fn(&str) -> bool) {
        let Ok(Json::Object(saved)) = serde_json::from_str::<Json>(text) else {
            return;
        };
        for (field, volume) in [
            ("music_volume", Volume::Music),
            ("sound_volume", Volume::Sound),
        ] {
            if let Some(value) = saved.get(field).and_then(saved_volume) {
                self.set_volume(volume, value);
            }
        }
        let Some(Json::Object(keys)) = saved.get("keys") else {
            return;
        };
        for (game_key, player_key) in keys {
            if let Some(player_key) = player_key
                .as_str()
                .filter(|key| !key.is_empty() && !reserved(key))
                && game_keys.iter().any(|key| key == game_key)
            {
                self.keys.assign(game_keys, game_key, player_key);
            }
        }
    }
}

fn saved_volume(value: &Json) -> Option<u8> {
    let number = value.as_f64()?;
    (number.fract() == 0.0 && (0.0..=f64::from(MAX_VOLUME)).contains(&number))
        .then_some(number as u8)
}

/// «Интерфейс игры» → «Настройки на экране»: название клавиши для игрока.
pub fn key_name(code: &str) -> String {
    let fixed = match code {
        "Space" => "Пробел",
        "Enter" => "Ввод",
        "ArrowLeft" => "Стрелка влево",
        "ArrowRight" => "Стрелка вправо",
        "ArrowUp" => "Стрелка вверх",
        "ArrowDown" => "Стрелка вниз",
        "ShiftLeft" => "Левый Shift",
        "ShiftRight" => "Правый Shift",
        "ControlLeft" => "Левый Ctrl",
        "ControlRight" => "Правый Ctrl",
        "AltLeft" => "Левый Alt",
        "AltRight" => "Правый Alt",
        _ => "",
    };
    if !fixed.is_empty() {
        return fixed.to_string();
    }
    let single = |rest: &str, expected: fn(char) -> bool| {
        let mut chars = rest.chars();
        match (chars.next(), chars.next()) {
            (Some(ch), None) if expected(ch) => Some(ch.to_string()),
            _ => None,
        }
    };
    code.strip_prefix("Key")
        .and_then(|rest| single(rest, |ch| ch.is_ascii_uppercase()))
        .or_else(|| {
            code.strip_prefix("Digit")
                .and_then(|rest| single(rest, |ch| ch.is_ascii_digit()))
        })
        .unwrap_or_else(|| code.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn keys(codes: &[&str]) -> Vec<String> {
        codes.iter().map(|code| (*code).to_string()).collect()
    }

    #[test]
    fn an_assigned_free_key_replaces_the_game_key_and_takes_it_away() {
        let game = keys(&["KeyA", "KeyJ"]);
        let mut replacements = KeyReplacements::default();
        replacements.assign(&game, "KeyA", "KeyU");
        assert_eq!(replacements.player_key("KeyA"), "KeyU");
        assert_eq!(replacements.world_key("KeyU"), Some("KeyA"));
        assert_eq!(replacements.world_key("KeyA"), None);
        assert_eq!(replacements.world_key("KeyJ"), Some("KeyJ"));
        assert_eq!(replacements.world_key("KeyZ"), Some("KeyZ"));
    }

    #[test]
    fn assigning_a_taken_key_swaps_the_two_game_keys() {
        let game = keys(&["Space", "KeyJ"]);
        let mut replacements = KeyReplacements::default();
        replacements.assign(&game, "KeyJ", "Space");
        assert_eq!(replacements.player_key("KeyJ"), "Space");
        assert_eq!(replacements.player_key("Space"), "KeyJ");
        assert_eq!(replacements.world_key("Space"), Some("KeyJ"));
        assert_eq!(replacements.world_key("KeyJ"), Some("Space"));
    }

    #[test]
    fn a_swap_hands_over_the_key_the_game_key_had_been_moved_to() {
        let game = keys(&["KeyA", "KeyJ"]);
        let mut replacements = KeyReplacements::default();
        replacements.assign(&game, "KeyA", "KeyU");
        replacements.assign(&game, "KeyJ", "KeyU");
        assert_eq!(replacements.player_key("KeyJ"), "KeyU");
        assert_eq!(replacements.player_key("KeyA"), "KeyJ");
    }

    #[test]
    fn assigning_the_game_keys_own_key_back_leaves_no_replacement() {
        let game = keys(&["KeyA", "KeyJ"]);
        let mut replacements = KeyReplacements::default();
        replacements.assign(&game, "KeyA", "KeyJ");
        replacements.assign(&game, "KeyA", "KeyA");
        assert_eq!(replacements, KeyReplacements::default());
    }

    #[test]
    fn a_freed_game_key_can_be_taken_by_another_game_key() {
        let game = keys(&["KeyA", "KeyJ"]);
        let mut replacements = KeyReplacements::default();
        replacements.assign(&game, "KeyA", "KeyU");
        replacements.assign(&game, "KeyJ", "KeyA");
        assert_eq!(replacements.world_key("KeyA"), Some("KeyJ"));
        assert_eq!(replacements.world_key("KeyJ"), None);
    }

    #[test]
    fn saved_settings_come_back_and_unsuitable_parts_are_dropped() {
        let game = keys(&["KeyA", "KeyJ"]);
        let mut settings = PlayerSettings::default();
        settings.load_json(
            r#"{"music_volume": 80, "sound_volume": 140,
                "keys": {"KeyA": "KeyU", "KeyQ": "KeyW", "KeyJ": "Escape", "Nope": 3}}"#,
            &game,
            |key| key == "Escape",
        );
        assert_eq!(settings.volume(Volume::Music), 80);
        assert_eq!(settings.volume(Volume::Sound), 100);
        assert_eq!(settings.keys.player_key("KeyA"), "KeyU");
        assert_eq!(settings.keys.player_key("KeyJ"), "KeyJ");
        assert_eq!(settings.keys.player_key("KeyQ"), "KeyQ");
    }

    #[test]
    fn a_fractional_or_negative_volume_and_broken_text_change_nothing() {
        let mut settings = PlayerSettings::default();
        settings.load_json(r#"{"music_volume": 50.5, "sound_volume": -1}"#, &[], |_| {
            false
        });
        settings.load_json("не json", &[], |_| false);
        settings.load_json("[1, 2]", &[], |_| false);
        assert_eq!(settings, PlayerSettings::default());
    }

    #[test]
    fn settings_survive_a_round_trip_through_json() {
        let game = keys(&["KeyA", "KeyJ"]);
        let mut settings = PlayerSettings::default();
        settings.set_volume(Volume::Music, 35);
        settings.keys.assign(&game, "KeyA", "KeyJ");
        let mut loaded = PlayerSettings::default();
        loaded.load_json(&settings.to_json(), &game, |_| false);
        assert_eq!(loaded, settings);
    }

    #[test]
    fn default_settings_serialise_with_every_field() {
        assert_eq!(
            PlayerSettings::default().to_json(),
            r#"{"keys":{},"music_volume":100,"sound_volume":100}"#
        );
    }

    #[test]
    fn set_volume_reports_a_change_and_clamps() {
        let mut settings = PlayerSettings::default();
        assert!(!settings.set_volume(Volume::Sound, 100));
        assert!(settings.set_volume(Volume::Sound, 40));
        assert!(!settings.set_volume(Volume::Music, 250));
        assert_eq!(settings.volume(Volume::Music), 100);
        assert_eq!(settings.volume(Volume::Sound), 40);
    }

    #[test]
    fn key_names_follow_the_russian_table() {
        let name = |code: &str| key_name(code);
        assert_eq!(name("KeyA"), "A");
        assert_eq!(name("Digit1"), "1");
        assert_eq!(name("Space"), "Пробел");
        assert_eq!(name("Enter"), "Ввод");
        assert_eq!(name("ArrowLeft"), "Стрелка влево");
        assert_eq!(name("ShiftRight"), "Правый Shift");
        assert_eq!(name("ControlLeft"), "Левый Ctrl");
        assert_eq!(name("AltRight"), "Правый Alt");
        assert_eq!(name("Tab"), "Tab");
        assert_eq!(name("Backspace"), "Backspace");
        assert_eq!(name("F5"), "F5");
        assert_eq!(name("KeyAB"), "KeyAB");
        assert_eq!(name("NumpadEnter"), "NumpadEnter");
    }
}
