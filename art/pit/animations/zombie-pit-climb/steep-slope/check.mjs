// Замер ног: исходный клип и клип с поправками из motions.json (как движок: локальный поворот × поправка).
// node check.mjs <glb> <клип> <motions.json> <движение> [шаг]
import fs from 'fs';
import { loadRig, qmul, sub, norm, angle, ground, eulerQuat } from './rig.mjs';

const [, , file, clipName, motionsFile, motion, stepArg = '0.1'] = process.argv;
const R = loadRig(file, clipName);
const bones = JSON.parse(fs.readFileSync(motionsFile, 'utf8'))[motion]?.bones ?? {};
const cr = (marks, phase) => {
  const last = marks.length - 1;
  if (phase <= marks[0].at) return marks[0].turn;
  if (phase >= marks[last].at) return marks[last].turn;
  const after = marks.findIndex((m) => m.at > phase);
  const p = (i) => marks[Math.max(0, Math.min(last, i))];
  const a = p(after - 2).turn, f = p(after - 1), to = p(after), c = p(after + 1).turn;
  const t = (phase - f.at) / Math.max(1e-6, to.at - f.at);
  return f.turn.map((x, k) => 0.5 * (2 * x + (-a[k] + to.turn[k]) * t + (2 * a[k] - 5 * x + 4 * to.turn[k] - c[k]) * t * t + (-a[k] + 3 * x - 3 * to.turn[k] + c[k]) * t * t * t));
};
const pose = (t, fixed) => {
  const L = R.locals(t);
  if (fixed) for (const [name, marks] of Object.entries(bones)) {
    const i = R.id(name.replace('mixamorig:', ''));
    const turn = Array.isArray(marks[0]) || typeof marks[0] === 'number' ? marks : cr(marks, t / R.len);
    L[i] = { ...L[i], r: qmul(L[i].r, eulerQuat(turn)) };
  }
  return R.globals(L);
};
const gap = (p) => p[1] - ground(p[2]);
const step = +stepArg;
const worst = { ankle: -1e9, toe: -1e9, knee: 1e9, ball: 1e9, tip: 1e9, heel: 1e9 };
for (const fixed of [false, true]) {
  const w = { ankle: -1e9, toe: -1e9, knee: 1e9, ball: 1e9, tip: 1e9 };
  const rows = [];
  for (let t = 0; t <= R.len + 1e-6; t += step) {
    const G = pose(t, fixed);
    let s = t.toFixed(2);
    for (const side of ['Left', 'Right']) {
      const P = (nm) => G[R.id(side + nm)].p;
      const ank = angle(sub(P('Foot'), P('Leg')), sub(P('ToeBase'), P('Foot')));
      const toe = angle(sub(P('ToeBase'), P('Foot')), sub(P('Toe_End'), P('ToeBase'))) - 35;
      w.ankle = Math.max(w.ankle, ank); w.toe = Math.max(w.toe, toe);
      w.knee = Math.min(w.knee, gap(P('Leg'))); w.ball = Math.min(w.ball, gap(P('ToeBase'))); w.tip = Math.min(w.tip, gap(P('Toe_End')));
      s += ` | ${side[0]} голеностоп ${ank.toFixed(0)} пальцы +${toe.toFixed(0)} колено ${gap(P('Leg')).toFixed(3)} лодыжка ${gap(P('Foot')).toFixed(3)} подушечка ${gap(P('ToeBase')).toFixed(3)} кончик ${gap(P('Toe_End')).toFixed(3)}`;
    }
    rows.push(s);
  }
  console.log(fixed ? '== с поправками' : '== исходный клип', JSON.stringify(Object.fromEntries(Object.entries(w).map(([k, v]) => [k, +v.toFixed(3)]))));
  if (process.env.ROWS) console.log(rows.join('\n'));
}
