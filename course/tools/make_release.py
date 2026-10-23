import glob, os, re, shutil, sys

snap = sys.argv[1]
skip = sys.argv[2:]
for c in skip:
    for base in ("exercises", "solutions"):
        shutil.rmtree(os.path.join(snap, base, c), ignore_errors=True)
for f in glob.glob(os.path.join(snap, "exercises/*/*/chapter.toml")):
    chdir = os.path.dirname(f)
    s = open(f, encoding="utf-8").read()
    blocks = re.split(r"(?=\n\[\[exercises\]\])", s)
    keep = []
    for b in blocks:
        m = re.search(r'name = "([^"]+)"', b)
        if m and 'kind = "milestone"' in b and not os.path.isdir(os.path.join(chdir, m[1])):
            print("drop milestone stub", m[1])
            continue
        keep.append(b)
    open(f, "w", encoding="utf-8").write("".join(keep))
