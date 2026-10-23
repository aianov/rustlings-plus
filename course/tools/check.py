import argparse
import json
import os
import re
import shutil
import signal
import socket
import subprocess
import sys
import tomllib

ROOT = os.path.dirname(os.path.dirname(os.path.dirname(os.path.abspath(__file__))))
COURSE = os.path.join(ROOT, "course")
WORK = os.path.join(ROOT, "target", "course-check")
CARGO_TARGET = os.path.join(WORK, "cargo-target")
LOCALES = ("en", "ru")
SERVICES = {
    "postgres": ("RUSTLINGS_PG_URL", "127.0.0.1", 55432),
    "scylla": ("RUSTLINGS_SCYLLA_URI", "127.0.0.1", 59042),
    "nats": ("RUSTLINGS_NATS_URL", "127.0.0.1", 54222),
    "redis": ("RUSTLINGS_REDIS_URL", "127.0.0.1", 56379),
}
EXERCISE_KEYS = {
    "name", "kind", "test", "strict_clippy", "skip_check_unsolved", "quiz", "requires",
    "input_files", "test_command", "locked", "hint", "journey", "style", "exam", "why",
}
QUESTION_KEYS = {"name", "kind", "quiz", "style", "question", "options", "answer", "answer_text", "explain", "hint"}
STYLES = {"fix", "todo", "scratch", "bughunt"}
MILESTONE_KEYS = {"name", "kind", "journey", "quiz"}
MILESTONE_TOML_KEYS = {"hint", "requires", "test_command", "story"}
RULE_FIELDS = {
    "exists": ({"paths"}, set()),
    "absent": ({"globs"}, set()),
    "uniform_modules": ({"roots"}, set()),
    "no_comments": ({"globs"}, set()),
    "workspace_members": ({"members"}, set()),
    "workspace_deps": ({"packages"}, set()),
    "deps": ({"package"}, {"forbid", "allow_only"}),
    "contains": ({"globs", "regex"}, set()),
    "not_contains": ({"globs", "regex"}, set()),
    "max_lines": ({"globs", "max"}, set()),
    "allowed_children": ({"dirs", "allowed"}, set()),
}
MS_JOURNEY = {}
MS_WHERE = {}
MS_INFO = {}
SNAPSHOTS = {}
ARCHCHECK = {}
NAME_RE = re.compile(r"^[a-z][a-z0-9_]*$")
VARIANT_RE = re.compile(r"^(?P<stem>.+)\.(?P<loc>en|ru)(?P<ext>\.[^.]+)$")
ALIGNED_EXT = (".rs", ".toml", ".proto", ".sql", ".cql")

errors = []
results = []


SCOPE = {"on": True}


def err(msg):
    if not SCOPE["on"]:
        print(f"WARN   (outside scope) {msg}", flush=True)
        return
    errors.append(msg)
    print(f"ERROR  {msg}", flush=True)


DASHES = ("\u2014", "\u2013")


VY_RE = re.compile(r"(?<![А-Яа-яЁё])(вы|вам|вас|вами|ваш|ваша|ваше|ваши|вашего|вашей|вашу|вашим|ваших)(?![А-Яа-яЁё])", re.IGNORECASE)


def vy_problem(line, rust):
    if rust:
        k = line.find("//")
        if k < 0:
            return False
        line = line[k:]
    line = re.sub(r"«[^»]*»", "", line)
    if rust:
        line = re.sub(r'"[^"]*"', "", line)
    return bool(VY_RE.search(line))


def dash_problem(text, markdown=False, rust=False):
    in_code = False
    for i, line in enumerate(text.split("\n"), 1):
        if markdown and line.lstrip().startswith("```"):
            in_code = not in_code
            continue
        if any(d in line for d in DASHES):
            return i
        if vy_problem(line, rust):
            return i
        if markdown and not in_code and " -- " in re.sub(r"`[^`]*`", "", line):
            return i
    return None


def style_file(path):
    try:
        text = open(path, encoding="utf-8").read()
    except (UnicodeDecodeError, OSError):
        return
    n = dash_problem(text, path.endswith(".md"), path.endswith(".rs"))
    if n:
        err(f"style: dash or «вы» on line {n} of {os.path.relpath(path, ROOT)}")


def style_tree(top, only_variants):
    for dp, dns, fns in os.walk(top):
        dns[:] = [d for d in dns if d != "target"]
        for f in fns:
            if not f.endswith((".rs", ".md", ".toml", ".proto", ".sql")):
                continue
            if only_variants and not VARIANT_RE.match(f):
                continue
            style_file(os.path.join(dp, f))


def style_text(where, v, only_ru):
    items = v.items() if isinstance(v, dict) else [("base", v)]
    for loc, t in items:
        for x in t if isinstance(t, list) else [t]:
            if isinstance(x, str) and (not only_ru or loc == "ru") and dash_problem(x):
                err(f"style: dash or «вы» in {where} [{loc}]")


def text_ok(v, allow_list=False, min_step=0):
    def one(x):
        if isinstance(x, str):
            return True
        return allow_list and isinstance(x, list) and x and all(isinstance(i, str) and len(i.strip()) >= min_step for i in x)
    if one(v):
        return True
    return isinstance(v, dict) and bool(v) and all(k in LOCALES and one(x) for k, x in v.items())


def variant_path(path, loc):
    d, f = os.path.split(path)
    stem, ext = os.path.splitext(f)
    return os.path.join(d, f"{stem}.{loc}{ext}")


def lines_of(path):
    with open(path, encoding="utf-8") as fh:
        return fh.read().split("\n")


def check_aligned(base, variant):
    if not base.endswith(ALIGNED_EXT):
        return
    a, b = lines_of(base), lines_of(variant)
    if len(a) != len(b):
        err(f"alignment: {os.path.relpath(variant, ROOT)} has {len(b)} lines, base has {len(a)}")


def file_variants(base):
    out = []
    for loc in LOCALES:
        p = variant_path(base, loc)
        if os.path.exists(p):
            check_aligned(base, p)
            out.append(loc)
    return out


def tree_variants(top):
    for dp, dns, fns in os.walk(top):
        dns[:] = [d for d in dns if d != "target"]
        for f in fns:
            m = VARIANT_RE.match(f)
            if m:
                base = os.path.join(dp, m["stem"] + m["ext"])
                if not os.path.exists(base):
                    err(f"variant without base file: {os.path.relpath(os.path.join(dp, f), ROOT)}")
                else:
                    check_aligned(base, os.path.join(dp, f))
            if f == "Cargo.lock":
                err(f"Cargo.lock must not be committed: {os.path.relpath(os.path.join(dp, f), ROOT)}")


def tree_locales(top):
    locs = set()
    for dp, dns, fns in os.walk(top):
        dns[:] = [d for d in dns if d != "target"]
        for f in fns:
            m = VARIANT_RE.match(f)
            if m:
                locs.add(m["loc"])
    return sorted(locs)


def load_course():
    with open(os.path.join(COURSE, "course.toml"), "rb") as fh:
        return tomllib.load(fh)


def lint(course, scope_track, scope_chapter):
    seen = {}
    plan = []
    pending = []
    for jr in course.get("journeys", []):
        if not isinstance(jr.get("id"), str) or not isinstance(jr.get("dir"), str):
            err("course.toml: journey needs string `id` and `dir`")
        if not text_ok(jr.get("title")):
            err(f"course.toml: journey {jr.get('id')}: bad title")
        for m in jr.get("milestones", []):
            if m in MS_JOURNEY:
                err(f"course.toml: milestone `{m}` listed twice")
            MS_JOURNEY[m] = jr
    for msg in ("welcome_message", "final_message"):
        if not text_ok(course.get(msg)):
            err(f"course.toml: bad {msg}")
    for track in course["tracks"]:
        tid = track["id"]
        for key in ("title", "description"):
            if not text_ok(track.get(key)):
                err(f"course.toml: track {tid}: bad {key}")
        for ch in track["chapters"]:
            cdir = os.path.join(ROOT, "exercises", tid, ch)
            sdir = os.path.join(ROOT, "solutions", tid, ch)
            in_scope = (scope_track in (None, tid)) and (scope_chapter in (None, f"{tid}/{ch}"))
            SCOPE["on"] = in_scope
            ctoml = os.path.join(cdir, "chapter.toml")
            if not os.path.exists(ctoml):
                if in_scope and scope_chapter:
                    err(f"{tid}/{ch}: chapter.toml missing")
                elif in_scope:
                    print(f"WARN   {tid}/{ch}: chapter not written yet")
                continue
            try:
                with open(ctoml, "rb") as fh:
                    info = tomllib.load(fh)
            except tomllib.TOMLDecodeError as e:
                err(f"{tid}/{ch}/chapter.toml: {e}")
                continue
            if not text_ok(info.get("chapter", {}).get("title")):
                err(f"{tid}/{ch}: [chapter] title missing")
            basics = tid == "rust-basics"
            style_text(f"{tid}/{ch} title", info.get("chapter", {}).get("title", ""), basics)
            for ex in info.get("exercises", []):
                style_text(f"{tid}/{ch}/{ex.get('name')} hint", ex.get("hint", ""), basics)
            for d in (cdir, sdir):
                if os.path.isdir(d):
                    style_tree(d, basics)
            if not os.path.exists(os.path.join(cdir, "README.md")):
                err(f"{tid}/{ch}: README.md missing")
            else:
                file_variants(os.path.join(cdir, "README.md"))
            exs = info.get("exercises", [])
            if not exs:
                err(f"{tid}/{ch}: no exercises")
            for ex in exs:
                name = ex.get("name", "?")
                where = f"{tid}/{ch}/{name}"
                if ex.get("kind") not in ("question", "milestone"):
                    for k in ex:
                        if k not in EXERCISE_KEYS:
                            err(f"{where}: unknown key `{k}`")
                if not NAME_RE.match(name):
                    err(f"{where}: bad name")
                if name in seen:
                    err(f"{where}: duplicate name (also {seen[name]})")
                seen[name] = where
                kind = ex.get("kind", "file")
                if kind != "milestone" and not text_ok(ex.get("hint"), allow_list=True, min_step=10):
                    if not (ex.get("exam") and "hint" not in ex):
                        err(f"{where}: hint missing")
                if ex.get("style") is not None and ex.get("style") not in STYLES:
                    err(f"{where}: bad style `{ex.get('style')}`")
                if "why" in ex and not text_ok(ex["why"]):
                    err(f"{where}: bad `why`")
                style_text(f"{where} why", ex.get("why", ""), tid == "rust-basics")
                for r in ex.get("requires", []):
                    if r not in SERVICES:
                        err(f"{where}: unknown service `{r}`")
                if kind == "file":
                    for k in ("test_command", "locked"):
                        if k in ex:
                            err(f"{where}: `{k}` is only for projects")
                    e, s = os.path.join(cdir, name + ".rs"), os.path.join(sdir, name + ".rs")
                    if not os.path.exists(e):
                        err(f"{where}: exercise file missing")
                        continue
                    if not os.path.exists(s):
                        err(f"{where}: solution file missing")
                        continue
                    locs = sorted(set(file_variants(e)) | set(file_variants(s)))
                    for f in ex.get("input_files", []):
                        if not os.path.exists(os.path.join(cdir, f)):
                            err(f"{where}: input file {f} missing")
                elif kind == "project":
                    e, s = os.path.join(cdir, name), os.path.join(sdir, name)
                    if not os.path.isdir(e) or not os.path.isdir(s):
                        err(f"{where}: project dir missing in exercises/ or solutions/")
                        continue
                    if not os.path.exists(os.path.join(e, "README.md")):
                        err(f"{where}: project README.md (ТЗ) missing")
                    tree_variants(e)
                    tree_variants(s)
                    for f in ex.get("locked", []):
                        pe, ps = os.path.join(e, f), os.path.join(s, f)
                        if not os.path.exists(pe) or not os.path.exists(ps):
                            err(f"{where}: locked file {f} missing")
                        elif open(pe, "rb").read() != open(ps, "rb").read():
                            err(f"{where}: locked file {f} differs between exercise and solution")
                    locs = sorted(set(tree_locales(e)) | set(tree_locales(s)))
                elif kind == "question":
                    for k in ex:
                        if k not in QUESTION_KEYS:
                            err(f"{where}: unknown key `{k}` for a question")
                    for k in ("question", "explain"):
                        if not text_ok(ex.get(k)):
                            err(f"{where}: `{k}` missing")
                        style_text(f"{where} {k}", ex.get(k, ""), tid == "rust-basics")
                    opts, ans, free = ex.get("options"), ex.get("answer"), ex.get("answer_text")
                    if free is not None:
                        if opts is not None or ans is not None or not (isinstance(free, list) and free and all(isinstance(a, str) for a in free)):
                            err(f"{where}: answer_text must be a non-empty list of strings, without options/answer")
                    else:
                        if not text_ok(opts, allow_list=True) or not isinstance(ans, list) or not ans:
                            err(f"{where}: needs options + answer (or answer_text)")
                        else:
                            lists = opts.values() if isinstance(opts, dict) else [opts]
                            lens = {len(o) for o in lists}
                            if len(lens) != 1:
                                err(f"{where}: options have different length per locale")
                            elif not all(isinstance(i, int) and 0 <= i < next(iter(lens)) for i in ans):
                                err(f"{where}: answer index out of range")
                    continue
                elif kind == "milestone":
                    for k in ex:
                        if k not in MILESTONE_KEYS:
                            err(f"{where}: unknown key `{k}` for a milestone (allowed: name, kind, journey, quiz)")
                    jr = MS_JOURNEY.get(name)
                    if ex.get("journey") not in [j.get("id") for j in course.get("journeys", [])]:
                        err(f"{where}: journey `{ex.get('journey')}` not in course.toml")
                    elif jr is None or jr.get("id") != ex.get("journey"):
                        err(f"{where}: milestone is not listed in journey `{ex.get('journey')}`")
                    MS_WHERE[name] = (tid, ch)
                    pending.append((track, ch, ex, in_scope))
                    continue
                else:
                    err(f"{where}: bad kind `{kind}`")
                    continue
                if in_scope:
                    plan.append((track, ch, ex, locs))
    for track, ch, ex, in_scope in pending:
        SCOPE["on"] = in_scope
        locs = lint_milestone(track["id"], ch, ex)
        if in_scope and locs is not None:
            plan.append((track, ch, ex, locs))
    SCOPE["on"] = True
    return plan


def toml_value(v):
    if isinstance(v, bool):
        return "true" if v else "false"
    if isinstance(v, (int, float)):
        return str(v)
    if isinstance(v, str):
        return '"' + v.replace("\\", "\\\\").replace('"', '\\"') + '"'
    if isinstance(v, list):
        return "[" + ", ".join(toml_value(x) for x in v) + "]"
    if isinstance(v, dict):
        return "{ " + ", ".join(f"{k} = {toml_value(x)}" for k, x in v.items()) + " }"
    raise ValueError(v)


def render_deps(deps):
    out = []
    for name, spec in deps.items():
        if isinstance(spec, dict) and spec.get("path") == "@support":
            spec = dict(spec, path=os.path.join(COURSE, "support"))
        out.append(f"{name} = {toml_value(spec)}")
    return "\n".join(out)


def service_up(name):
    env, host, port = SERVICES[name]
    val = os.environ.get(env)
    if val:
        m = re.search(r"([\w.\-]+):(\d+)", val.split("@")[-1])
        if m:
            host, port = m[1], int(m[2])
    try:
        with socket.create_connection((host, port), timeout=1):
            return True
    except OSError:
        return False


def run(cmd, cwd, timeout=900):
    conf = os.path.join(WORK, "clippy-conf")
    os.makedirs(conf, exist_ok=True)
    if not os.path.exists(os.path.join(conf, "clippy.toml")):
        open(os.path.join(conf, "clippy.toml"), "w").close()
    env = dict(os.environ, CARGO_TARGET_DIR=CARGO_TARGET, CARGO_TERM_COLOR="never", RUST_BACKTRACE="0", CLIPPY_CONF_DIR=conf)
    p = subprocess.Popen(cmd, cwd=cwd, env=env, stdout=subprocess.PIPE, stderr=subprocess.STDOUT,
                         text=True, start_new_session=True)
    try:
        out, _ = p.communicate(timeout=timeout)
        return p.returncode, out
    except subprocess.TimeoutExpired:
        try:
            os.killpg(p.pid, signal.SIGKILL)
        except ProcessLookupError:
            pass
        out, _ = p.communicate()
        return 124, (out or "") + f"\n[timeout after {timeout}s: whole process group killed]"


def tail(out, n=40):
    return "\n".join(out.rstrip().split("\n")[-n:])


def report(status, what, out=""):
    results.append((status, what))
    print(f"{status:<6} {what}", flush=True)
    if status == "FAIL" and out:
        print("       | " + tail(out).replace("\n", "\n       | "), flush=True)


def wanted_locales(locs, arg):
    if arg == "all":
        return ["base"] + locs
    return [arg if arg in locs else "base"]


def write_manifest(track, ch, pkg, bins):
    tid = track["id"]
    pkg_name = ("check-" + tid + "-" + ch).replace("_", "-")
    manifest = f'[package]\nname = "{pkg_name}"\nversion = "0.0.0"\nedition = "2024"\npublish = false\n\n[workspace]\n\n'
    manifest += "[dependencies]\n" + render_deps(track.get("dependencies", {})) + "\n\n" + "\n".join(bins)
    with open(os.path.join(pkg, "Cargo.toml"), "w") as fh:
        fh.write(manifest)


def cargo_json(args, pkg, timeout=900):
    args = list(args)
    k = args.index("--") if "--" in args else len(args)
    args.insert(k, "--message-format=json")
    code, out = run(["cargo", *args], pkg, timeout=timeout)
    exes, errs, rendered = {}, set(), {}
    for line in out.split("\n"):
        if not line.startswith("{"):
            continue
        try:
            m = json.loads(line)
        except ValueError:
            continue
        if m.get("reason") == "compiler-artifact" and m.get("executable"):
            exes[m["target"]["name"]] = m["executable"]
        elif m.get("reason") == "compiler-message" and m["message"].get("level") == "error":
            name = m["target"]["name"]
            errs.add(name)
            rendered[name] = rendered.get(name, "") + (m["message"].get("rendered") or "")
    return code, out, exes, errs, rendered


def run_exe(exe, pkg, test, timeout=180):
    return run([exe, "--quiet"] if test else [exe], pkg, timeout=timeout)


def check_file_chapter(track, ch, items, loc_arg):
    tid = track["id"]
    cdir = os.path.join(ROOT, "exercises", tid, ch)
    sdir = os.path.join(ROOT, "solutions", tid, ch)
    pkg = os.path.join(WORK, "pkg", tid, ch)
    os.makedirs(pkg, exist_ok=True)
    bins, jobs = [], []
    for ex, locs in items:
        name = ex["name"]
        for loc in wanted_locales(locs, loc_arg):
            suffix = "" if loc == "base" else f".{loc}"
            for role, d in (("ex", cdir), ("sol", sdir)):
                p = os.path.join(d, f"{name}{suffix}.rs")
                if not os.path.exists(p):
                    p = os.path.join(d, f"{name}.rs")
                bins.append(f'[[bin]]\nname = "{name}_{loc}_{role}"\npath = {toml_value(p)}\n')
            jobs.append((ex, loc))
        for f in ex.get("input_files", []):
            shutil.copy(os.path.join(cdir, f), os.path.join(pkg, f))
    write_manifest(track, ch, pkg, bins)
    live = []
    for ex, loc in jobs:
        down = [r for r in ex.get("requires", []) if not service_up(r)]
        if down:
            report("SKIP", f"{tid}/{ch}/{ex['name']} [{loc}]: service down: {', '.join(down)}")
        else:
            live.append((ex, loc))
    if not live:
        return
    def sol(ex, loc):
        return f"{ex['name']}_{loc}_sol"
    def exb(ex, loc):
        return f"{ex['name']}_{loc}_ex"
    tests = [(e, l) for e, l in live if e.get("test", True)]
    runs = [(e, l) for e, l in live if not e.get("test", True)]
    exes = {}
    for group, verb in ((tests, ["test", "--no-run"]), (runs, ["build"])):
        if not group:
            continue
        flags = [x for e, l in group for x in ("--bin", sol(e, l))]
        code, out, got, _, _ = cargo_json([*verb, *flags], pkg)
        if code != 0:
            print("       | batch build of solutions failed, falling back to per-exercise checks", flush=True)
            return check_file_chapter_slow(track, ch, items, loc_arg)
        exes.update(got)
    failed = set()
    for e, l in live:
        code, out = run_exe(exes[sol(e, l)], pkg, e.get("test", True))
        if code != 0:
            report("FAIL", f"{tid}/{ch}/{e['name']} [{l}]: solution does not pass", out)
            failed.add((e["name"], l))
    clippy = ["clippy", "--profile", "test"]
    for deny in (True, False):
        group = [(e, l) for e, l in live if (e["name"], l) not in failed and bool(e.get("strict_clippy") or tid != "rust-basics") == deny]
        if not group:
            continue
        flags = [x for e, l in group for x in ("--bin", sol(e, l))]
        code, out, _, errs, rendered = cargo_json([*clippy, "--keep-going", *flags, *(["--", "-D", "warnings"] if deny else [])], pkg)
        for e, l in group:
            if sol(e, l) in errs:
                report("FAIL", f"{tid}/{ch}/{e['name']} [{l}]: solution has clippy warnings", rendered.get(sol(e, l), out))
                failed.add((e["name"], l))
        if code != 0 and not errs:
            report("FAIL", f"{tid}/{ch}: clippy batch failed", out)
            return
    todo = [(e, l) for e, l in live if (e["name"], l) not in failed and not e.get("skip_check_unsolved")]
    for e, l in live:
        if (e["name"], l) not in failed and e.get("skip_check_unsolved"):
            report("OK", f"{tid}/{ch}/{e['name']} [{l}]")
    if not todo:
        return
    flags = [x for e, l in todo for x in ("--bin", exb(e, l))]
    _, _, _, broken, _ = cargo_json(["check", "--keep-going", "--profile", "test", *flags], pkg)
    compiling = [(e, l) for e, l in todo if exb(e, l) not in broken]
    ex_exes = {}
    for group, verb in (([p for p in compiling if p[0].get("test", True)], ["test", "--no-run"]),
                        ([p for p in compiling if not p[0].get("test", True)], ["build"])):
        if not group:
            continue
        flags = [x for e, l in group for x in ("--bin", exb(e, l))]
        code, out, got, _, _ = cargo_json([*verb, *flags], pkg)
        ex_exes.update(got)
    for e, l in todo:
        what = f"{tid}/{ch}/{e['name']} [{l}]"
        if exb(e, l) in broken:
            report("OK", what)
            continue
        exe = ex_exes.get(exb(e, l))
        if exe is None:
            report("OK", what)
            continue
        code, out = run_exe(exe, pkg, e.get("test", True))
        if code == 0 and e.get("strict_clippy"):
            code, out = run(["cargo", *clippy, "--quiet", "--bin", exb(e, l), "--", "-D", "warnings"], pkg)
        if code == 0:
            report("FAIL", f"{what}: exercise already passes (must fail until solved)", out)
        else:
            report("OK", what)



def check_file_chapter_slow(track, ch, items, loc_arg, pkg_ready=False):
    tid = track["id"]
    cdir = os.path.join(ROOT, "exercises", tid, ch)
    sdir = os.path.join(ROOT, "solutions", tid, ch)
    pkg = os.path.join(WORK, "pkg", tid, ch)
    os.makedirs(pkg, exist_ok=True)
    bins = []
    jobs = []
    for ex, locs in items:
        name = ex["name"]
        for loc in wanted_locales(locs, loc_arg):
            suffix = "" if loc == "base" else f".{loc}"
            for role, d in (("ex", cdir), ("sol", sdir)):
                p = os.path.join(d, f"{name}{suffix}.rs")
                if not os.path.exists(p):
                    p = os.path.join(d, f"{name}.rs")
                b = f"{name}_{loc}_{role}"
                bins.append(f'[[bin]]\nname = "{b}"\npath = {toml_value(p)}\n')
            jobs.append((ex, loc))
        for f in ex.get("input_files", []):
            shutil.copy(os.path.join(cdir, f), os.path.join(pkg, f))
    write_manifest(track, ch, pkg, bins)
    for ex, loc in jobs:
        name = ex["name"]
        what = f"{tid}/{ch}/{name} [{loc}]"
        down = [r for r in ex.get("requires", []) if not service_up(r)]
        if down:
            report("SKIP", f"{what}: service down: {', '.join(down)}")
            continue
        verb = ["test"] if ex.get("test", True) else ["run"]
        sol = f"{name}_{loc}_sol"
        code, out = run(["cargo", *verb, "--quiet", "--bin", sol], pkg)
        if code != 0:
            report("FAIL", f"{what}: solution does not pass", out)
            continue
        deny = ex.get("strict_clippy") or tid != "rust-basics"
        clippy = ["cargo", "clippy", "--quiet", "--profile", "test", "--bin"]
        code, out = run([*clippy, sol, *(["--", "-D", "warnings"] if deny else [])], pkg)
        if code != 0:
            report("FAIL", f"{what}: solution has clippy warnings", out)
            continue
        if not ex.get("skip_check_unsolved"):
            code, out = run(["cargo", *verb, "--quiet", "--bin", f"{name}_{loc}_ex"], pkg)
            if code == 0 and ex.get("strict_clippy"):
                code, out = run([*clippy, f"{name}_{loc}_ex", "--", "-D", "warnings"], pkg)
            if code == 0:
                report("FAIL", f"{what}: exercise already passes (must fail until solved)", out)
                continue
        report("OK", what)


def materialize(src, dst, loc):
    if os.path.exists(dst):
        shutil.rmtree(dst)
    for dp, dns, fns in os.walk(src):
        dns[:] = [d for d in dns if d != "target"]
        rel = os.path.relpath(dp, src)
        out_dir = os.path.normpath(os.path.join(dst, rel))
        os.makedirs(out_dir, exist_ok=True)
        for f in fns:
            m = VARIANT_RE.match(f)
            if m:
                if m["loc"] != loc:
                    continue
                shutil.copy(os.path.join(dp, f), os.path.join(out_dir, m["stem"] + m["ext"]))
            else:
                stem, ext = os.path.splitext(f)
                if loc != "base" and os.path.exists(os.path.join(dp, f"{stem}.{loc}{ext}")):
                    continue
                shutil.copy(os.path.join(dp, f), os.path.join(out_dir, f))


def check_project(track, ch, ex, locs, loc_arg):
    tid, name = track["id"], ex["name"]
    cmd = ex.get("test_command", ["cargo", "test", "--workspace"])
    for loc in wanted_locales(locs, loc_arg):
        what = f"{tid}/{ch}/{name} [{loc}]"
        down = [r for r in ex.get("requires", []) if not service_up(r)]
        if down:
            report("SKIP", f"{what}: service down: {', '.join(down)}")
            continue
        base = os.path.join(WORK, "proj", name)
        sol_dir, ex_dir = os.path.join(base, f"sol-{loc}"), os.path.join(base, f"ex-{loc}")
        materialize(os.path.join(ROOT, "solutions", tid, ch, name), sol_dir, loc)
        code, out = run(cmd, sol_dir, timeout=1800)
        if code != 0:
            report("FAIL", f"{what}: solution does not pass `{' '.join(cmd)}`", out)
            continue
        code, out = run(["cargo", "clippy", "--quiet", "--workspace", "--all-targets", "--", "-D", "warnings"], sol_dir, timeout=1800)
        if code != 0:
            report("FAIL", f"{what}: solution has clippy warnings", out)
            continue
        if not ex.get("skip_check_unsolved"):
            materialize(os.path.join(ROOT, "exercises", tid, ch, name), ex_dir, loc)
            code, out = run(cmd, ex_dir, timeout=1800)
            if code == 0:
                report("FAIL", f"{what}: exercise project already passes (must fail until solved)", out)
                continue
        report("OK", what)


def read_tree(top):
    out = {}
    if not os.path.isdir(top):
        return out
    for dp, dns, fns in os.walk(top):
        dns[:] = [d for d in dns if d != "target"]
        for f in fns:
            full = os.path.join(dp, f)
            with open(full, "rb") as fh:
                out[os.path.relpath(full, top).replace(os.sep, "/")] = fh.read()
    return out


def prev_milestone(name):
    ms = MS_JOURNEY[name]["milestones"]
    i = ms.index(name)
    return ms[i - 1] if i else None


def overlay_dir(name):
    tid, ch = MS_WHERE[name]
    return os.path.join(ROOT, "solutions", tid, ch, name)


def deleted_paths(files):
    raw = files.get(".deleted")
    if raw is None:
        return []
    return [l.strip().strip("/") for l in raw.decode("utf-8").split("\n") if l.strip()]


def snapshot(name):
    if name in SNAPSHOTS:
        return SNAPSHOTS[name]
    result = None
    if name in MS_WHERE and os.path.isdir(overlay_dir(name)):
        prev = prev_milestone(name)
        base = {} if prev is None else snapshot(prev)
        if base is not None:
            files = dict(base)
            overlay = read_tree(overlay_dir(name))
            for p in deleted_paths(overlay):
                for k in [k for k in files if k == p or k.startswith(p + "/")]:
                    del files[k]
            files.update({k: v for k, v in overlay.items() if k != ".deleted"})
            result = files
    SNAPSHOTS[name] = result
    return result


def milestone_cmd(name):
    cmd = MS_INFO.get(name, {}).get("test_command", ["cargo", "test", "--workspace"])
    return cmd.split() if isinstance(cmd, str) else cmd


def lint_arch(where, path):
    try:
        with open(path, "rb") as fh:
            doc = tomllib.load(fh)
    except tomllib.TOMLDecodeError as e:
        err(f"{where}: arch.toml: {e}")
        return
    rules = doc.get("rules")
    if not isinstance(rules, list) or not rules:
        err(f"{where}: arch.toml: no [[rules]]")
        return
    for i, r in enumerate(rules, 1):
        kind = r.get("kind")
        if kind not in RULE_FIELDS:
            err(f"{where}: arch.toml rule #{i}: unknown kind `{kind}`")
            continue
        req, opt = RULE_FIELDS[kind]
        for k in req:
            if k not in r:
                err(f"{where}: arch.toml rule #{i} ({kind}): missing `{k}`")
        for k in r:
            if k not in req | opt | {"kind", "message"}:
                err(f"{where}: arch.toml rule #{i} ({kind}): unknown field `{k}`")
        if kind == "deps" and "forbid" not in r and "allow_only" not in r:
            err(f"{where}: arch.toml rule #{i} (deps): need `forbid` or `allow_only`")
        if not text_ok(r.get("message")):
            err(f"{where}: arch.toml rule #{i} ({kind}): message missing")
        else:
            style_text(f"{where} arch.toml rule #{i} message", r["message"], False)


def lint_milestone(tid, ch, ex):
    name = ex["name"]
    where = f"{tid}/{ch}/{name}"
    edir = os.path.join(ROOT, "exercises", tid, ch, name)
    sdir = overlay_dir(name)
    if not os.path.isdir(edir):
        print(f"WARN   {where}: milestone not written yet (journey agent writes milestones in order)", flush=True)
        return None
    if not os.path.isdir(sdir):
        err(f"{where}: solution overlay dir missing in solutions/")
        return None
    if not os.path.exists(os.path.join(edir, "README.md")):
        err(f"{where}: milestone README.md missing")
    if not os.path.exists(os.path.join(edir, "arch.toml")):
        err(f"{where}: arch.toml missing")
    else:
        lint_arch(where, os.path.join(edir, "arch.toml"))
    mt = os.path.join(edir, "milestone.toml")
    info = {}
    if os.path.exists(mt):
        try:
            with open(mt, "rb") as fh:
                info = tomllib.load(fh)
        except tomllib.TOMLDecodeError as e:
            err(f"{where}: milestone.toml: {e}")
        for k in info:
            if k not in MILESTONE_TOML_KEYS:
                err(f"{where}: milestone.toml: unknown key `{k}`")
        for k in ("hint", "story"):
            if k in info:
                if not text_ok(info[k], allow_list=(k == "hint")):
                    err(f"{where}: milestone.toml: bad {k}")
                else:
                    style_text(f"{where} milestone.toml {k}", info[k], tid == "rust-basics")
        for r in info.get("requires", []):
            if r not in SERVICES:
                err(f"{where}: milestone.toml: unknown service `{r}`")
        tc = info.get("test_command")
        if tc is not None and not (isinstance(tc, str) or (isinstance(tc, list) and tc and all(isinstance(x, str) for x in tc))):
            err(f"{where}: milestone.toml: bad test_command")
    MS_INFO[name] = info
    tree_variants(edir)
    tree_variants(sdir)
    if os.path.exists(os.path.join(edir, "README.md")):
        file_variants(os.path.join(edir, "README.md"))
    overlay = read_tree(sdir)
    prev = prev_milestone(name)
    base = {} if prev is None else snapshot(prev)
    if base is not None:
        for p in deleted_paths(overlay):
            if not any(k == p or k.startswith(p + "/") for k in base):
                err(f"{where}: .deleted path `{p}` not in the previous snapshot")
    inject = read_tree(os.path.join(edir, "inject"))
    snap = snapshot(name)
    if snap is not None:
        for rel, data in inject.items():
            if rel not in snap:
                err(f"{where}: inject file {rel} missing from the solution snapshot")
            elif snap[rel] != data:
                err(f"{where}: inject file {rel} differs from the solution snapshot")
    locs = set(tree_locales(edir)) | set(tree_locales(sdir))
    for rel in (snap or {}):
        m = VARIANT_RE.match(os.path.basename(rel))
        if m:
            locs.add(m["loc"])
    return sorted(locs)


def archcheck_bin():
    if "bin" not in ARCHCHECK:
        code, out = run(["cargo", "build", "--quiet", "--manifest-path", os.path.join(COURSE, "archcheck", "Cargo.toml")], ROOT)
        ARCHCHECK["bin"] = os.path.join(CARGO_TARGET, "debug", "archcheck") if code == 0 else None
        ARCHCHECK["out"] = out
    return ARCHCHECK["bin"]


def write_files(files, dst):
    if os.path.exists(dst):
        shutil.rmtree(dst)
    os.makedirs(dst)
    for rel, data in files.items():
        if rel == ".deleted":
            continue
        out = os.path.join(dst, rel)
        os.makedirs(os.path.dirname(out), exist_ok=True)
        with open(out, "wb") as fh:
            fh.write(data)


def run_archcheck(binary, proj, arch, loc):
    lang = loc if loc in LOCALES else "ru"
    return run([binary, proj, arch, "--locale", lang], ROOT)


def check_milestone(track, ch, ex, locs, loc_arg):
    tid, name = track["id"], ex["name"]
    cmd = milestone_cmd(name)
    requires = MS_INFO.get(name, {}).get("requires", [])
    prev = prev_milestone(name)
    sol = snapshot(name)
    base = {} if prev is None else snapshot(prev)
    if sol is None or base is None:
        missing = prev if base is None else name
        report("SKIP", f"{tid}/{ch}/{name}: snapshot of milestone `{missing}` not written yet")
        return
    edir = os.path.join(ROOT, "exercises", tid, ch, name)
    inject = read_tree(os.path.join(edir, "inject"))
    scaffold = read_tree(os.path.join(edir, "scaffold"))
    arch = os.path.join(edir, "arch.toml")
    binary = archcheck_bin()
    for loc in wanted_locales(locs, loc_arg):
        what = f"{tid}/{ch}/{name} [{loc}]"
        if binary is None:
            report("FAIL", f"{what}: archcheck does not build", ARCHCHECK["out"])
            continue
        down = [r for r in requires if not service_up(r)]
        if down:
            report("SKIP", f"{what}: service down: {', '.join(down)}")
            continue
        root = os.path.join(WORK, "proj", name)
        sol_dir = os.path.join(root, f"sol-{loc}")
        write_files({**sol, **inject}, os.path.join(root, f"raw-sol-{loc}"))
        materialize(os.path.join(root, f"raw-sol-{loc}"), sol_dir, loc)
        code, out = run_archcheck(binary, sol_dir, arch, loc)
        if code != 0:
            report("FAIL", f"{what}: solution violates arch.toml", out)
            continue
        code, out = run(cmd, sol_dir, timeout=1800)
        if code != 0:
            report("FAIL", f"{what}: solution does not pass `{' '.join(cmd)}`", out)
            continue
        code, out = run(["cargo", "clippy", "--quiet", "--workspace", "--all-targets", "--", "-D", "warnings"], sol_dir, timeout=1800)
        if code != 0:
            report("FAIL", f"{what}: solution has clippy warnings", out)
            continue
        start = dict(base)
        for rel, data in scaffold.items():
            start.setdefault(rel, data)
        start.update(inject)
        ex_dir = os.path.join(root, f"ex-{loc}")
        write_files(start, os.path.join(root, f"raw-ex-{loc}"))
        materialize(os.path.join(root, f"raw-ex-{loc}"), ex_dir, loc)
        code, out = run_archcheck(binary, ex_dir, arch, loc)
        if code == 2:
            report("FAIL", f"{what}: archcheck error on the starting state", out)
            continue
        if code == 0:
            code, out = run(cmd, ex_dir, timeout=1800)
            if code == 0:
                report("FAIL", f"{what}: starting state already passes archcheck and tests (must fail until solved)", out)
                continue
        report("OK", what)


def main():
    ap = argparse.ArgumentParser(description="Course content checker")
    ap.add_argument("--track")
    ap.add_argument("--chapter", help="<track>/<chapter>")
    ap.add_argument("--only", action="append", help="exercise name (repeatable)")
    ap.add_argument("--locale", default="all", choices=["all", "base", *LOCALES])
    ap.add_argument("--lint", action="store_true", help="format checks only, no builds")
    a = ap.parse_args()
    track = a.track or (a.chapter.split("/")[0] if a.chapter else None)
    plan = lint(load_course(), track, a.chapter)
    if a.only:
        plan = [p for p in plan if p[2]["name"] in a.only]
    if a.lint:
        print(f"\nlint: {len(errors)} error(s), {len(plan)} exercise(s) in scope")
        sys.exit(1 if errors else 0)
    by_chapter = {}
    for tr, ch, ex, locs in plan:
        if ex.get("kind", "file") == "project":
            check_project(tr, ch, ex, locs, a.locale)
        elif ex.get("kind") == "milestone":
            check_milestone(tr, ch, ex, locs, a.locale)
        else:
            by_chapter.setdefault((tr["id"], ch), (tr, []))[1].append((ex, locs))
    for (tid, ch), (tr, items) in by_chapter.items():
        check_file_chapter(tr, ch, items, a.locale)
    fails = sum(1 for s, _ in results if s == "FAIL")
    skips = sum(1 for s, _ in results if s == "SKIP")
    oks = sum(1 for s, _ in results if s == "OK")
    print(f"\nsummary: {oks} ok, {fails} fail, {skips} skip, {len(errors)} format error(s)")
    sys.exit(1 if fails or errors else 0)


if __name__ == "__main__":
    main()
