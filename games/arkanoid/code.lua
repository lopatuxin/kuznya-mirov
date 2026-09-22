-- Отскок мяча от ракетки: угол зависит только от места удара, не от движения ракетки.
function paddle_bounce(ball, paddle)
  if ball.velocity.y < 0 then
    return
  end

  local ball_mid = ball.position.x + ball.size.x / 2
  local paddle_mid = paddle.position.x + paddle.size.x / 2
  local offset = (ball_mid - paddle_mid) / (paddle.size.x / 2)
  if offset > 1 then
    offset = 1
  elseif offset < -1 then
    offset = -1
  end

  local angle = offset * (60 * math.pi / 180)
  local speed = math.sqrt(
    ball.velocity.x * ball.velocity.x + ball.velocity.y * ball.velocity.y
  )
  ball.velocity.x = speed * math.sin(angle)
  ball.velocity.y = -speed * math.cos(angle)
  ball.position.y = paddle.position.y - ball.size.y
end

-- Прочный кирпич: удар считает hits и на третьем разбивается — «Арканоид», требование 51.
function strong_brick_hit(ball, brick)
  brick.hits = brick.hits + 1
  play_sound("brick")
  if brick.hits >= 3 then
    local scorers = find({ has = { "score" } })
    for i = 1, #scorers do
      scorers[i].score = scorers[i].score + 10
    end
    delete(brick)
  end
end
