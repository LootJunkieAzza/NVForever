"""Exact-text edits that keep a file's line endings: ed.py FILE OLDFILE NEWFILE."""
import sys


def edit(p, old, new, count=1):
    s = open(p, encoding='utf-8', newline='').read()
    nl = '\r\n' if '\r\n' in s else '\n'
    old = old.replace('\r\n', '\n').replace('\n', nl)
    new = new.replace('\r\n', '\n').replace('\n', nl)
    n = s.count(old)
    if n == 0:
        raise SystemExit(f'{p}: not found: {old[:80]!r}')
    s = s.replace(old, new, count)
    open(p, 'w', encoding='utf-8', newline='').write(s)


if __name__ == '__main__':
    edit(sys.argv[1], open(sys.argv[2], encoding='utf-8').read(), open(sys.argv[3], encoding='utf-8').read())
