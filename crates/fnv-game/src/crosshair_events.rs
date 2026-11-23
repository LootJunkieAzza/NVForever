//! Activation-range crosshair event delivery; see ext-events.md.
use crate::game::Game;
use fnv_script::vm::XVal;
use fnv_sim::FormId;

impl Game<'_> {
    /// Publish the activation ray's current reference. JohnnyGuitar retains its
    /// existing before-publication callback; OnCrosshairOff (for the previous
    /// reference) and OnCrosshairOn (for the new reference) see the current getter.
    pub fn set_crosshair_ref(&mut self, now: Option<FormId>) {
        let prev = self.senses.crosshair_ref;
        let changed = now != prev;
        if let Some(r) = now.filter(|_| changed) {
            self.jg8_crosshair(r);
        }
        self.senses.crosshair_ref = now;
        self.sync_pc_target();
        if !changed {
            return;
        }
        if let Some(old) = prev {
            self.dispatch_crosshair_event("OnCrosshairOff", "oncrosshairoff", old);
        }
        if let Some(r) = now {
            self.dispatch_crosshair_event("OnCrosshairOn", "oncrosshairon", r);
        }
    }

    fn dispatch_crosshair_event(&mut self, event: &str, lower: &str, target: FormId) {
        if !self.handlers.iter().any(|h| {
            h.event == lower
                || (h.event.starts_with(lower)
                    && h.event.as_bytes().get(lower.len()) == Some(&b':'))
        }) {
            return;
        }
        // Snapshot the type before callbacks can change the reference's data.
        let ty = crate::functions::base_of(self, target)
            .and_then(|base| self.data.sig(base))
            .and_then(|sig| fnv_script::vm::form_type_code(&sig));
        self.dispatch_event(event, vec![XVal::Ref(target.0)], Some(target));
        if let Some(ty) = ty {
            self.dispatch_event(
                &format!("{event}:{ty}"),
                vec![XVal::Ref(target.0)],
                Some(target),
            );
        }
    }
}
