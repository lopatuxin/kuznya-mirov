import type { StandBone } from "./useAnimationStand";

export type BoneRow = { index: number; name: string; depth: number; hasChildren: boolean };

/** Строки дерева костей сверху вниз: корни и дети раскрытых костей, у каждой глубина вложенности. */
export function visibleBoneRows(bones: readonly StandBone[], expanded: ReadonlySet<number>): BoneRow[] {
  const children = new Map<number, number[]>();
  bones.forEach((bone, index) => children.set(bone.parent, [...(children.get(bone.parent) ?? []), index]));
  const rows: BoneRow[] = [];
  function visit(index: number, depth: number): void {
    const own = children.get(index) ?? [];
    rows.push({ index, name: (bones[index] as StandBone).name, depth, hasChildren: own.length > 0 });
    if (!expanded.has(index)) return;
    for (const child of own) visit(child, depth + 1);
  }
  for (const root of children.get(-1) ?? []) visit(root, 0);
  return rows;
}

/** Кости от корня до родителя кости `index`: раскрыв их, кость видна в дереве. */
export function boneAncestors(bones: readonly StandBone[], index: number): number[] {
  const ancestors: number[] = [];
  for (let parent = bones[index]?.parent ?? -1; parent >= 0 && !ancestors.includes(parent); parent = bones[parent]?.parent ?? -1) ancestors.unshift(parent);
  return ancestors;
}

/** Имя кости без префикса скелета (`mixamorig:Hips` → `Hips`): в узкой панели важна вторая половина. */
export function shortBoneName(name: string): string {
  const colon = name.lastIndexOf(":");
  return colon >= 0 ? name.slice(colon + 1) : name;
}
