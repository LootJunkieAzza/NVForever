//! The extender's note functions: JIP's GetNoteRead and JohnnyGuitar's note getters and
//! setters (#984) [documented: each page; docs/spec-notes/holotapes.md §5]. The setters
//! change the note for the session ([`Game::note_edits`], not saved); SetNoteRead with
//! `serialize` 1 changes the saved read flag.

use crate::data::NoteDef;
use crate::game::Game;
use fnv_script::vm::{ArrayKind, XKey, XVal};
use fnv_sim::FormId;

fn form(args: &[XVal], k: usize) -> Option<FormId> {
    match args.get(k)? {
        XVal::Ref(0) => None,
        XVal::Ref(r) => Some(FormId(*r)),
        _ => None,
    }
}

fn num(args: &[XVal], k: usize) -> f64 {
    match args.get(k) {
        Some(XVal::Num(n)) => *n,
        _ => 0.0,
    }
}

fn text(args: &[XVal], k: usize) -> String {
    match args.get(k) {
        Some(XVal::Str(s)) => s.clone(),
        _ => String::new(),
    }
}

fn reff(f: Option<FormId>) -> XVal {
    XVal::Ref(f.map_or(0, |f| f.0))
}

impl Game<'_> {
    /// A note as it is now: the session's edits, else the record.
    pub fn note(&self, id: FormId) -> Option<&NoteDef> {
        self.note_edits.get(&id).or_else(|| self.data.notes.get(&id))
    }

    /// What a note plays now ([`crate::holotapes::tape_of`]), the session's edits applied.
    pub fn note_tape(&self, id: FormId) -> Option<crate::holotapes::Tape> {
        crate::holotapes::tape_of(self.data, self.note(id)?)
    }

    /// Whether the player has read a note: SetNoteRead's session change, else the saved
    /// flag ([`Game::mark_note_read`]).
    pub fn note_read(&self, id: FormId) -> bool {
        self.note_read_session.get(&id).copied().unwrap_or_else(|| self.player.notes_read.contains(&id))
    }

    /// The player reads a note in the Pip-Boy: its page is shown (the engine's DATA page,
    /// Notes) [documented: GetNoteRead, "has been read in the Pip-Boy"; hypothesis: shown
    /// counts as read, as for a Sound or Voice note there is nothing else to do]. Saved.
    pub fn mark_note_read(&mut self, id: FormId) {
        self.note_read_session.remove(&id);
        self.player.notes_read.insert(id);
    }

    /// Change a note for the session, starting from what it is now; false for a form
    /// that is not a note.
    fn edit_note(&mut self, id: FormId, f: impl FnOnce(&mut NoteDef)) -> bool {
        let Some(mut n) = self.note(id).cloned() else { return false };
        f(&mut n);
        self.note_edits.insert(id, n);
        true
    }
}

fn yes(b: bool) -> XVal {
    XVal::Num(f64::from(u8::from(b)))
}

/// The functions of this file; `None` for any other name.
pub(crate) fn call(g: &mut Game, name: &str, _subject: Option<FormId>, args: &[XVal]) -> Option<Result<XVal, String>> {
    let note = form(args, 0);
    let def = note.and_then(|n| g.note(n));
    let v = match name {
        // [documented: GetNoteRead] 0 for a form that is not a note [hypothesis].
        "getnoteread" => yes(note.is_some_and(|n| def.is_some() && g.note_read(n))),
        // DATA [documented: GetNoteType, 0 Sound, 1 Text, 2 Image, 3 Voice].
        "getnotetype" => XVal::Num(def.map_or(0.0, |d| f64::from(d.kind))),
        // SNAM, one field for both [documented: xEdit, a union by type; hypothesis: each
        // getter reads it whatever the type, as the field is one].
        "getnotesound" | "getnotespeaker" => reff(def.and_then(|d| d.sound.or(d.speaker))),
        // TNAM of a Voice note [documented: GetNoteTopic; a Text note's TNAM is its text].
        "getnotetopic" => reff(def.and_then(|d| d.topic)),
        // XNAM [documented: GetNoteImage]; "" without one.
        "getnoteimage" => XVal::Str(def.and_then(|d| d.texture.clone()).unwrap_or_default()),
        // ONAM, in record order [documented: GetNoteQuestList, "an array containing all
        // quests"].
        "getnotequestlist" => {
            let qs: Vec<(XKey, XVal)> = def.map(|d| d.quests.clone()).unwrap_or_default().into_iter().enumerate().map(|(i, q)| (XKey::Num(i as f64), XVal::Ref(q.0))).collect();
            XVal::Array(g.ext.new_array(ArrayKind::Array, qs))
        }
        // The setters: 1 when the form is a note, 0 otherwise [hypothesis: "int" on each
        // page; not probed]. A type outside 0-3 is refused [hypothesis].
        "setnotetype" => {
            let k = num(args, 1);
            let ok = (0.0..=3.0).contains(&k) && note.is_some_and(|n| g.edit_note(n, |d| d.kind = k as u8));
            yes(ok)
        }
        "setnotesound" => {
            let s = form(args, 1);
            yes(note.is_some_and(|n| {
                g.edit_note(n, |d| {
                    d.sound = s;
                    d.speaker = None;
                })
            }))
        }
        "setnotespeaker" => {
            let s = form(args, 1);
            yes(note.is_some_and(|n| {
                g.edit_note(n, |d| {
                    d.speaker = s;
                    d.sound = None;
                })
            }))
        }
        "setnotetopic" => {
            let t = form(args, 1);
            yes(note.is_some_and(|n| g.edit_note(n, |d| d.topic = t)))
        }
        "setnoteimage" => {
            let t = text(args, 1);
            yes(note.is_some_and(|n| g.edit_note(n, |d| d.texture = Some(t).filter(|t| !t.is_empty()))))
        }
        // [documented: AddNoteQuest, RemoveNoteQuest] a quest already there is not added
        // twice [hypothesis].
        "addnotequest" => {
            let q = form(args, 1);
            yes(match (note, q) {
                (Some(n), Some(q)) => g.edit_note(n, |d| {
                    if !d.quests.contains(&q) {
                        d.quests.push(q);
                    }
                }),
                _ => false,
            })
        }
        "removenotequest" => {
            let q = form(args, 1);
            let had = def.is_some_and(|d| q.is_some_and(|q| d.quests.contains(&q)));
            if let (true, Some(n), Some(q)) = (had, note, q) {
                g.edit_note(n, |d| d.quests.retain(|x| *x != q));
            }
            yes(had)
        }
        // [documented: SetNoteRead] `serialize` 1 keeps the change in the save; without
        // it the change lasts for the session. Returns nothing ("void").
        "setnoteread" => {
            if let Some(n) = note.filter(|_| def.is_some()) {
                let read = num(args, 1) != 0.0;
                if num(args, 2) != 0.0 {
                    g.note_read_session.remove(&n);
                    if read {
                        g.player.notes_read.insert(n);
                    } else {
                        g.player.notes_read.remove(&n);
                    }
                } else {
                    g.note_read_session.insert(n, read);
                }
            }
            XVal::Num(0.0)
        }
        _ => return None,
    };
    Some(Ok(v))
}
