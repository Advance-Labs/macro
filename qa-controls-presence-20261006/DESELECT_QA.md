# Background deselection browser QA

Separate scratch form01a11273-9918-7c9d-a5f5-0721d8135079 only; retained Forms playground and event demo untouched.

Passed: edit question title without blur, click empty desktop viewport, editor collapses and server GET confirms title saved. Help input stays expanded. More-actions menu opens and closing it leaves question selected. Select section then background removes focused border. Add screener, click background, rules editor/Done closes together with question selection. Mobile390x844 touch: edit title then tap empty left gutter, editor collapses and server GET confirms new title persisted.

No desktop/mobile page errors. A temporary Vite merge-conflict overlay interrupted the first pass; checks were repeated after the rebase was resolved. Screenshots: deselect-title-saved.png, deselect-screener-closed.png, deselect-mobile-after.png.
