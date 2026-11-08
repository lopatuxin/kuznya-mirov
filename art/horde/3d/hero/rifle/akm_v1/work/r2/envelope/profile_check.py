from mathutils.bvhtree import BVHTree

obj = bpy.data.objects[names[0]]
bm = bmesh.new()
bm.from_mesh(obj.data)
bad = [e for e in bm.edges if len(e.link_faces) != 2]
bad_details = [{"faces":len(e.link_faces), "pixel": [(v.co.y * PPM + ANCHOR_PX[0], ANCHOR_PX[1] - v.co.z * PPM) for v in e.verts], "coords":[list(v.co) for v in e.verts]} for e in bad]
bm.free()
tree = BVHTree.FromObject(obj, depsgraph_of([obj]))
samples = {}
mask = load_rgba(REFERENCE)[:,:,3] > 0.5
for label, x, top, distances in (("receiver",1300,84,[0.5,2,4,7,10,14,18,25]),("handguard",800,77,[0.5,2,4,6,8,10,15]),("stock",1900,161,[0.5,2,4,6,8,10,15])):
    top = int(np.nonzero(mask[:,x])[0].min())
    rows=[]
    for mm in distances:
        y = top + mm * 0.001 * PPM
        origin = Vector((0.2,(x-ANCHOR_PX[0])/PPM,(ANCHOR_PX[1]-y)/PPM))
        hit=tree.ray_cast(origin,Vector((-1,0,0)))
        rows.append({"inset_mm":mm,"width_mm":float(hit[0].x*2000) if hit[0] else None})
    samples[label]=rows
cam=aim(sc,[obj],_dirv(90,50),(1600,700))
render_objs(sc,[obj],cam,(1600,700),OUT+r"\rounded_top_clay.png")
result={"bad_edges":bad_details,"sections":samples,"tris":len(obj.data.polygons)}
