"""Deploy the puzzle-answers ingest worker (ES module + D1 binding + vars)."""
import sys, json, uuid
sys.path.insert(0, "/home/hatch/workspace/substitution-solver/cloudflare")
from cf import api

AID = "8aee88d9ea2ea8e660a82a12ce8fd47f"
D1_UUID = "40a46205-2aa4-4d26-8bd4-dd8614ac0225"
WORKER = "puzzle-answers"
DIR = "/home/hatch/workspace/substitution-solver/cloudflare"
OWNER = sys.argv[1] if len(sys.argv) > 1 else ""
REPO = sys.argv[2] if len(sys.argv) > 2 else ""

with open(f"{DIR}/answers_worker.js", "rb") as f:
    script = f.read()
metadata = {
    "main_module": "answers_worker.js",
    "compatibility_date": "2024-06-01",
    "bindings": [{"type": "d1", "name": "DB", "id": D1_UUID}],
    "vars": {"GITHUB_OWNER": OWNER, "GITHUB_REPO": REPO},
}
boundary = "----cfworker" + uuid.uuid4().hex
body = b""
body += f"--{boundary}\r\n".encode()
body += b'Content-Disposition: form-data; name="metadata"\r\n'
body += b"Content-Type: application/json\r\n\r\n"
body += json.dumps(metadata).encode() + b"\r\n"
body += f"--{boundary}\r\n".encode()
body += b'Content-Disposition: form-data; name="answers_worker.js"; filename="answers_worker.js"\r\n'
body += b"Content-Type: application/javascript+module\r\n\r\n"
body += script + b"\r\n"
body += f"--{boundary}--\r\n".encode()
r = api("PUT", f"/accounts/{AID}/workers/scripts/{WORKER}",
        raw_bytes=body, content_type=f"multipart/form-data; boundary={boundary}")
print("deployed:", str(r)[:300])
