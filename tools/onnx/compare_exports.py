import numpy as np, onnxruntime as ort, os
from transformers import AutoTokenizer
REV="5c38ec7c405ec4b44b94cc5a9bb96e735b38267a"
tok=AutoTokenizer.from_pretrained("BAAI/bge-small-en-v1.5", revision=REV)
H=os.path.expanduser("~/models/onnx/")
texts=["The quick brown fox jumps over the lazy dog.",
       "Vector databases index embeddings for retrieval."]
def emb(path, text, L=None):
    s=ort.InferenceSession(path, providers=["CPUExecutionProvider"])
    names={i.name for i in s.get_inputs()}
    kw=dict(return_tensors="np", truncation=True)
    if L: kw.update(padding="max_length", max_length=L)
    enc=tok([text], **kw)
    feed={k:v.astype(np.int64) for k,v in enc.items() if k in names}
    out=s.run(None, feed)[0]
    m=enc["attention_mask"].astype(np.float32)[...,None]
    v=(out*m).sum(1)/np.maximum(m.sum(1),1e-9)
    return (v/np.linalg.norm(v,axis=1,keepdims=True))[0]
A=[emb(H+"bge-small-en-v1.5.onnx",t) for t in texts]
B=[emb(H+"bge-small-en-v1.5-opset17-static.onnx",t,512) for t in texts]
print("opset11(dynamic)  vs  opset17(static,1x512) -- same weights, different graph")
for i,t in enumerate(texts):
    print(f"  [{i}] cos={float(A[i]@B[i]):.9f}  max_diff={float(np.abs(A[i]-B[i]).max()):.3e}  {t[:42]}")
print()
print(f"  semantic margin  opset11 cos(t0,t1)={float(A[0]@A[1]):.6f}   opset17 cos(t0,t1)={float(B[0]@B[1]):.6f}")
print(f"  dim={A[0].shape[0]}  finite={bool(np.isfinite(A[0]).all() and np.isfinite(B[0]).all())}")
