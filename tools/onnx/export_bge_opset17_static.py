import torch, os
from transformers import AutoModel
REV = "5c38ec7c405ec4b44b94cc5a9bb96e735b38267a"
mid = "BAAI/bge-small-en-v1.5"
base = AutoModel.from_pretrained(mid, revision=REV, torch_dtype=torch.float32).eval()

class Wrap(torch.nn.Module):
    def __init__(self, m): super().__init__(); self.m = m
    def forward(self, input_ids, attention_mask, token_type_ids):
        return self.m(input_ids=input_ids, attention_mask=attention_mask,
                      token_type_ids=token_type_ids).last_hidden_state

model = Wrap(base).eval()
B, L = 1, 512
dummy = (torch.ones(B, L, dtype=torch.long),
         torch.ones(B, L, dtype=torch.long),
         torch.zeros(B, L, dtype=torch.long))
with torch.no_grad():
    ref = model(*dummy)
print("forward ok, output", tuple(ref.shape), ref.dtype)
out = os.path.expanduser("~/models/onnx/bge-small-en-v1.5-opset17-static.onnx")
torch.onnx.export(
    model, dummy, out,
    input_names=["input_ids","attention_mask","token_type_ids"],
    output_names=["last_hidden_state"],
    opset_version=17, dynamo=False, do_constant_folding=True,
)
print("wrote", out, os.path.getsize(out), "bytes")
