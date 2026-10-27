//! Общее для проверок скелетных моделей (`phase27_skeleton.rs`, `rpg_game.rs`): где стоит точка, привязанная к
//! суставу, в позе, которую движок отдал видеокарте.

#![allow(dead_code)]

use engine::core::math3::Mat4;

/// Точка связывания `bind` сустава, перенесённая матрицей сустава `matrix` в позу.
pub fn posed_point(matrix: &Mat4, bind: [f32; 3]) -> [f32; 3] {
    std::array::from_fn(|row| {
        matrix[0][row] * bind[0]
            + matrix[1][row] * bind[1]
            + matrix[2][row] * bind[2]
            + matrix[3][row]
    })
}

/// То же по матрицам суставов объекта, как они лежат в кадре (`ModelDraws::bones`): по шестнадцать чисел на сустав,
/// столбцами.
pub fn joint_point(bones: &[f32], joint: usize, bind: [f32; 3]) -> [f32; 3] {
    let matrix: Mat4 = std::array::from_fn(|column| {
        std::array::from_fn(|row| bones[joint * 16 + column * 4 + row])
    });
    posed_point(&matrix, bind)
}
