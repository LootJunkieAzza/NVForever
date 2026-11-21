//! Source filtering requests for existing DDS samplers (texture-filtering.md).
use crate::Game;
use fnv_script::vm::XVal;

impl Game<'_> {
    /// Consume the latest bounded source request. Renderer policy is application-local,
    /// not save data; restoring a Game does not replay an old request.
    pub fn take_texture_filter_level(&mut self) -> Option<u16> {
        self.other.texture_filter_level.take()
    }
}

pub(crate) fn call(g: &mut Game<'_>, name: &str, args: &[XVal]) -> Option<Result<XVal, String>> {
    if name != "setmaxaniso" {
        return None;
    }
    Some(match args {
        [XVal::Num(level)]
            if level.is_finite()
                && level.fract() == 0.0
                && [1.0, 2.0, 4.0, 8.0, 16.0].contains(level) =>
        {
            g.other.texture_filter_level = Some(*level as u16);
            Ok(XVal::Num(0.0))
        }
        _ => Err("SetMaxAniso supports one of the mapped filtering levels 1, 2, 4, 8, 16".into()),
    })
}

#[cfg(test)]
mod tests;
