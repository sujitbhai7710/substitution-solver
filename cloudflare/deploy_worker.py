"""Deploy the test worker (ES module + D1 binding) and manage its cron schedule."""
import sys, json, uuid
sys.path.insert(0, "/home/hatch/workspace/substitution-solver/cloudflare")
from cf import api

AID = "8aee88d9ea2ea8e660a82a12ce8fd47f"
D1_UUID = "40a46205-2aa4-4d26-8bd4-dd8614ac0225"
WORKER = "puzzle-scraper-test"
DIR = "/home/hatch/workspace/substitution-solver/cloudflare"


def deploy():
    with open(f"{DIR}/worker.js", "rb") as f:
        script = f.read()
    metadata = {
        "main_module": "worker.js",
        "compatibility_date": "2024-06-01",
        "bindings": [{"type": "d1", "name": "DB", "id": D1_UUID}],
    }
    boundary = "----cfworker" + uuid.uuid4().hex
    body = b""
    body += f"--{boundary}\r\n".encode()
    body += b'Content-Disposition: form-data; name="metadata"\r\n'
    body += b"Content-Type: application/json\r\n\r\n"
    body += json.dumps(metadata).encode() + b"\r\n"
    body += f"--{boundary}\r\n".encode()
    body += b'Content-Disposition: form-data; name="worker.js"; filename="worker.js"\r\n'
    body += b"Content-Type: application/javascript+module\r\n\r\n"
    body += script + b"\r\n"
    body += f"--{boundary}--\r\n".encode()
    r = api(
        "PUT",
        f"/accounts/{AID}/workers/scripts/{WORKER}",
        raw_bytes=body,
        content_type=f"multipart/form-data; boundary={boundary}",
    )
    print("deployed:", json.dumps(r, indent=1)[:800])


def set_cron(cron):
    # Schedules API takes a bare JSON array, not {"schedules": [...]}
    r = api(
        "PUT",
        f"/accounts/{AID}/workers/scripts/{WORKER}/schedules",
        [{"cron": cron}],
    )
    print("cron set:", json.dumps(r)[:500])


def get_cron():
    r = api("GET", f"/accounts/{AID}/workers/scripts/{WORKER}/schedules")
    print(json.dumps(r, indent=1)[:800])


def delete():
    r = api("DELETE", f"/accounts/{AID}/workers/scripts/{WORKER}")
    print("deleted:", json.dumps(r)[:300])


if __name__ == "__main__":
    {"deploy": deploy, "set-cron": lambda: set_cron(sys.argv[2]),
     "get-cron": get_cron, "delete": delete}[sys.argv[1]]()
