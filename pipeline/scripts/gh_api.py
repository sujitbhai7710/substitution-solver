#!/usr/bin/env python3
"""gh_api.py — minimal GitHub API client using the custom.github connector.

Usage:
  gh_api.py get <path>                      # GET repos/{owner}/{repo}/<path>
  gh_api.py push <message> <file>...        # commit files to master
  gh_api.py ref                             # print master SHA

Auth via dynamic_credentials surrogate (custom.github), same pattern as the
cloudflare skill. Only talks to api.github.com.
"""
import base64
import json
import os
import sys
import urllib.request

sys.path.insert(0, "/opt/hatch/skills/skill-creator/bin")
from dynamic_credentials import add_surrogate_to_request, read_json_response

OWNER = "sujitbhai7710"
REPO = "substitution-solver"
API = "https://api.github.com"
ALLOWED = ("api.github.com",)


def req(method, path, body=None):
    r = urllib.request.Request(API + path, method=method,
                               data=json.dumps(body).encode() if body is not None else None,
                               headers={"Accept": "application/vnd.github+json"})
    add_surrogate_to_request(r, "custom.github", allowed_hosts=ALLOWED)
    return read_json_response(urllib.request.urlopen(r, timeout=60))


def get_user():
    return req("GET", "/user")


def get_ref():
    return req("GET", f"/repos/{OWNER}/{REPO}/git/ref/heads/master")


def push(message, files):
    """Commit local files (paths relative to repo root) to master."""
    root = os.path.join(os.path.dirname(os.path.abspath(__file__)), "..", "..")
    ref = get_ref()
    base_sha = ref["object"]["sha"]
    base_commit = req("GET", f"/repos/{OWNER}/{REPO}/git/commits/{base_sha}")
    base_tree = base_commit["tree"]["sha"]
    tree_items = []
    for f in files:
        ap = os.path.join(root, f)
        with open(ap, "rb") as fh:
            content = fh.read()
        try:
            text = content.decode("utf-8")
            blob = req("POST", f"/repos/{OWNER}/{REPO}/git/blobs",
                       {"content": text, "encoding": "utf-8"})
        except UnicodeDecodeError:
            blob = req("POST", f"/repos/{OWNER}/{REPO}/git/blobs",
                       {"content": base64.b64encode(content).decode(), "encoding": "base64"})
        tree_items.append({"path": f, "mode": "100644", "type": "blob",
                           "sha": blob["sha"]})
    tree = req("POST", f"/repos/{OWNER}/{REPO}/git/trees",
               {"base_tree": base_tree, "tree": tree_items})
    commit = req("POST", f"/repos/{OWNER}/{REPO}/git/commits",
                 {"message": message, "tree": tree["sha"], "parents": [base_sha]})
    upd = req("PATCH", f"/repos/{OWNER}/{REPO}/git/refs/heads/master",
              {"sha": commit["sha"]})
    return {"commit": commit["sha"], "ref": upd["object"]["sha"]}


if __name__ == "__main__":
    cmd = sys.argv[1]
    if cmd == "user":
        u = get_user()
        print(json.dumps({"login": u["login"]}))
    elif cmd == "ref":
        print(get_ref()["object"]["sha"])
    elif cmd == "push":
        print(json.dumps(push(sys.argv[2], sys.argv[3:])))
    else:
        sys.exit("unknown cmd")
