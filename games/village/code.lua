-- Точка осмотра не выходит за край сцены: код размера сцены не видит, он записан здесь.
local SCENE_WIDTH = 160
local SCENE_HEIGHT = 24

function keep_in_scene(viewer)
  viewer.position.x = math.max(0, math.min(viewer.position.x, SCENE_WIDTH - viewer.size.x))
  viewer.position.y = math.max(0, math.min(viewer.position.y, SCENE_HEIGHT - viewer.size.y))
end
