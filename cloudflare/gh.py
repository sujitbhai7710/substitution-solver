"""Cloudflare API helper using the stored custom.github credential."""
import sys, json, urllib.request

sys.path.insert(0, "/opt/hatch/skills/skill-creator/bin")
from dynamic_credentials import (
    add_surrogate_to_request,
    read_json_response,
)

API = "https://api.github.com/client/v4"


def api(method, path, payload=None, raw_bytes=None, content_type=None):
    url = API + path
    data = None
    headers = {}
    if payload is not None:
        data = json.dumps(payload).encode()
        headers["Content-Type"] = "application/json"
    if raw_bytes is not None:
        data = raw_bytes
        if content_type:
            headers["Content-Type"] = content_type
    req = urllib.request.Request(url, data=data, headers=headers, method=method)
    add_surrogate_to_request(req, "custom.github", allowed_hosts=["api.github.com"])
    resp = urllib.request.urlopen(req, timeout=60)
    ctype = resp.headers.get("Content-Type", "")
    if "json" in ctype:
        body = read_json_response(resp)
        if isinstance(body, dict) and not body.get("success", True):
            raise RuntimeError(f"API error {path}: {body.get('errors')}")
        return body.get("result", body) if isinstance(body, dict) else body
    return resp.read()


if __name__ == "__main__":
    import sys as _s

    cmd = _s.argv[1]
    if cmd == "account":
        r = api("GET", "/accounts")
        for a in r:
            print(a["id"], "|", a["name"])
    elif cmd == "subdomain":
        aid = _s.argv[2]
        try:
            r = api("GET", f"/accounts/{aid}/workers/subdomain")
            print("subdomain:", r)
        except Exception as e:
            print("subdomain check failed:", e)
    elif cmd == "d1_create":
        aid, name = _s.argv[2], _s.argv[3]
        r = api("POST", f"/accounts/{aid}/d1/database", {"name": name})
        print(json.dumps(r, indent=1))
    elif cmd == "d1_list":
        aid = _s.argv[2]
        r = api("GET", f"/accounts/{aid}/d1/database")
        for d in r:
            print(d["uuid"], "|", d["name"])
    elif cmd == "d1_query":
        aid, uuid = _s.argv[2], _s.argv[3]
        sql = _s.argv[4]
        params = json.loads(_s.argv[5]) if len(_s.argv) > 5 else []
        r = api(
            "POST",
            f"/accounts/{aid}/d1/database/{uuid}/query",
            {"sql": sql, "params": params},
        )
        print(json.dumps(r, indent=1)[:3000])
