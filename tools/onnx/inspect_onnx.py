import onnx, collections, sys
from onnx import shape_inference
p = sys.argv[1]
m = onnx.load(p)
print("=== MODEL ===")
print("ir_version      ", m.ir_version)
print("producer        ", m.producer_name, m.producer_version)
for oi in m.opset_import:
    print("opset           ", repr(oi.domain or "ai.onnx"), "=", oi.version)
print()
print("=== INPUTS ===")
def tstr(v):
    t = v.type.tensor_type
    dims = [(d.dim_param or str(d.dim_value)) for d in t.shape.dim]
    return f"{v.name}: {onnx.TensorProto.DataType.Name(t.elem_type)}[{', '.join(dims)}]"
for v in m.graph.input:  print(" ", tstr(v))
print("=== OUTPUTS ===")
for v in m.graph.output: print(" ", tstr(v))
print()
# initializer dtypes -> is this fp32?
dt = collections.Counter(onnx.TensorProto.DataType.Name(i.data_type) for i in m.graph.initializer)
print("=== INITIALIZER DTYPES ===", dict(dt))
nparams = sum(1 for _ in m.graph.initializer)
import numpy as np
tot = 0
for i in m.graph.initializer:
    n = 1
    for d in i.dims: n *= d
    tot += n
print(f"initializers: {nparams}, total elements: {tot:,}")
print()
print("=== OPERATOR HISTOGRAM (whole graph, incl. subgraphs) ===")
c = collections.Counter()
doms = collections.Counter()
def walk(g):
    for n in g.node:
        c[n.op_type] += 1
        doms[n.domain or "ai.onnx"] += 1
        for a in n.attribute:
            if a.g.ByteSize(): walk(a.g)
            for sg in a.graphs: walk(sg)
walk(m.graph)
for op, n in sorted(c.items(), key=lambda kv: (-kv[1], kv[0])):
    print(f"  {op:<28} {n:>5}")
print(f"  {'TOTAL':<28} {sum(c.values()):>5}   distinct op types: {len(c)}")
print("  domains:", dict(doms))
