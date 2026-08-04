import onnx, collections, sys
m = onnx.load(sys.argv[1])
ops = collections.Counter(n.op_type for n in m.graph.node)

# Ryzen AI 1.8.0 ops_support table, transcribed from audit doc §3.1
TABLE = {
 "MatMul":("Y","Y","Y","Y"), "Add":("Y","Y","Y","Y"), "Transpose":("Y","Y","Y","Y"),
 "Reshape":("Y","Y","Y","Y"), "Slice":("Y","Y","Y","Y"), "Gemm":("Y","Y","Y","Y"),
 "Sqrt":("Y","Y","Y","Y"), "Sub":("Y","Y","Y","Y"), "Div":("Y","Y","Y","Y"),
 "Erf":("Y","-","-","Y"), "Gather":("Y","-","-","-"),
 "LayerNormalization":("-","-","-","Y"), "Softmax":("-","Y","Y","Y"), "Gelu":("-","Y","Y","Y"),
}
print(f"{'op':<16}{'count':>6}  {'BF16':<6}{'A16W8':<7}{'A8W8':<6}{'XINT8':<6} note")
print("-"*72)
unlisted=[]
for op,n in sorted(ops.items(), key=lambda kv:(-kv[1],kv[0])):
    if op in TABLE:
        b,a16,a8,x = TABLE[op]
        note = "" if b=="Y" else "*** NOT in BF16 column ***"
        print(f"{op:<16}{n:>6}  {b:<6}{a16:<7}{a8:<6}{x:<6} {note}")
    else:
        unlisted.append((op,n))
print()
print("NOT PRESENT IN THE TABLE AT ALL (neither supported nor refused):")
for op,n in unlisted: print(f"  {op:<16}{n:>6}")
print()
print("=== STRUCTURAL CHECKS ===")
for probe in ["LayerNormalization","Gelu","Tanh","Attention","MultiHeadAttention",
              "SkipLayerNormalization","EmbedLayerNormalization","FastGelu",
              "Expand","Where","Equal","Clip","Einsum"]:
    print(f"  {probe:<26} {'PRESENT x'+str(ops[probe]) if ops[probe] else 'absent'}")
print()
# static vs dynamic
print("=== SHAPE DYNAMISM ===")
for v in list(m.graph.input)+list(m.graph.output):
    dims=[(d.dim_param or d.dim_value) for d in v.type.tensor_type.shape.dim]
    dyn=[d for d in dims if isinstance(d,str)]
    print(f"  {v.name:<20} {dims}  {'DYNAMIC: '+','.join(dyn) if dyn else 'static'}")
shape_machinery = sum(ops[o] for o in ("Shape","Concat","Unsqueeze","Reshape","Gather","Slice","Cast"))
print(f"\n  shape-manipulation nodes (Shape/Concat/Unsqueeze/Reshape/Gather/Slice/Cast): {shape_machinery}")
print(f"  compute nodes (MatMul/Softmax/Erf): {ops['MatMul']+ops['Softmax']+ops['Erf']}")
print(f"  total nodes: {sum(ops.values())}")
