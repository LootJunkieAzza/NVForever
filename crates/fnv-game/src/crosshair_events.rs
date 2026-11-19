//! Activation-range crosshair event delivery; see ext-events.md.
use crate::game::Game;
use fnv_script::vm::XVal;
use fnv_sim::FormId;

impl Game<'_> {
    /// Publish the activation ray's current reference. JohnnyGuitar retains its
    /// existing before-publication callback; OnCrosshairOn sees the current getter.
    pub fn set_crosshair_ref(&mut self, now: Option<FormId>) {
        let changed = now != self.senses.crosshair_ref;
        if let Some(r) = now.filter(|_| changed) {
            self.jg8_crosshair(r);
        }
        self.senses.crosshair_ref = now;
        let Some(r) = now.filter(|_| changed) else {
            return;
        };
        if !self
            .handlers
            .iter()
            .any(|h| h.event == "oncrosshairon" || h.event.starts_with("oncrosshairon:"))
        {
            return;
        }
        // Snapshot the type before callbacks can change the reference's data.
        let ty = crate::functions::base_of(self, r)
            .and_then(|base| self.data.sig(base))
            .and_then(|sig| fnv_script::vm::form_type_code(&sig));
        self.dispatch_event("OnCrosshairOn", vec![XVal::Ref(r.0)], Some(r));
        if let Some(ty) = ty {
            self.dispatch_event(
                &format!("OnCrosshairOn:{ty}"),
                vec![XVal::Ref(r.0)],
                Some(r),
            );
        }
    }
}
