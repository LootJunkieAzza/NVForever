//! What Modern Minimap's map window asks of its plugin (ModernMinimap.dll) and what the
//! plugin writes back (docs/spec-notes/menu-xml.md §11). The renderer
//! (`nvforever::minimap_render`) draws the picture; this reads and writes the tiles.
//!
//! The mod's menu file names the contract in its comments [observed:
//! menus\prefabs\stentorious\modernminimap\main.xml, "ModernMinimap.dll Traits"]: while
//! `_Render` reads true (the picture's centre has left the window, the mod's scripts reset
//! `_MapOriginX` to -999999, or `_ForceUpdate`), the plugin renders the place from straight
//! above, `_FrustumSize` game units each way round the player (`_PlrX`/`_PlrY`/`_PlrZ`,
//! which the mod's render-update handler writes, OnRender.gek) from `_CameraOffset` over the
//! player's height, into the `MapRender` image, and writes the render's centre to
//! `_MapOriginX`/`_MapOriginY`; the file places the picture and every icon from those.

use fnv_ui::Ui;

/// The mod's map window in HUDMainMenu.
pub const ROOT: &str = "HUDMainMenu/ModernMinimap";

/// A render the map's tiles ask for: where (game units) and how wide.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Request {
    /// The player's position, game x, y, z.
    pub centre: [f32; 3],
    /// Half the picture's width, game units (`_FrustumSize`).
    pub half: f32,
    /// The camera's height over the player, game units (`_CameraOffset`).
    pub offset: f32,
}

/// The render the tiles ask for this frame, if any: `_Render` true, with the player's
/// position and a positive, finite frustum.
pub fn request(ui: &Ui) -> Option<Request> {
    let get = |t: &str| ui.get_float(&format!("{ROOT}/{t}")).filter(|v| v.is_finite());
    if get("_Render").unwrap_or(0.0) == 0.0 {
        return None;
    }
    let half = get("_FrustumSize").filter(|h| *h > 0.0)? as f32;
    let centre = [get("_PlrX")? as f32, get("_PlrY")? as f32, get("_PlrZ").unwrap_or(0.0) as f32];
    Some(Request { centre, half, offset: get("_CameraOffset").unwrap_or(10000.0) as f32 })
}

/// After a render: the picture's centre for the file's placement, and the picture's name
/// (a runtime picture, `super::runtime_images`) in place of the file's placeholder.
pub fn rendered(ui: &mut Ui, r: &Request, filename: &str) {
    ui.set_float(&format!("{ROOT}/_MapOriginX"), f64::from(r.centre[0]));
    ui.set_float(&format!("{ROOT}/_MapOriginY"), f64::from(r.centre[1]));
    ui.set_string(&format!("{ROOT}/MapRender/filename"), filename);
}

#[cfg(test)]
mod tests {
    use super::*;
    use fnv_ui::MemSource;

    /// The mod's window as its file lays the traits out (the formulas cut down to the
    /// ones read here: `_Render` from `_ForceUpdate` and the picture's offset), settled
    /// once as the HUD's tree is every frame before the renderer reads it.
    fn ui(render: &str, plr: [f64; 2]) -> Ui {
        let path = r"menus\main\hud_main_menu.xml";
        let file = format!(
            r#"<menu name="HUDMainMenu"> <rect name="ModernMinimap">
            <_ForceUpdate> 0 </_ForceUpdate> <_MapOriginX> -999999 </_MapOriginX> <_MapOriginY> 0 </_MapOriginY>
            <_PlrX> {} </_PlrX> <_PlrY> {} </_PlrY> <_PlrZ> 300 </_PlrZ>
            <_Frustum> 2048 </_Frustum> <_FrustumSize> <copy src="me()" trait="_Frustum" /> </_FrustumSize>
            <_CameraOffset> 10000 </_CameraOffset>
            <_Render> {render} </_Render>
            <image name="MapRender"> <filename> Interface\Shared\solid.dds </filename> </image>
            </rect> </menu>"#,
            plr[0], plr[1]
        );
        let src = MemSource::default().with(path, &file);
        let mut ui = Ui::new();
        ui.load_menu(&src, path).unwrap();
        ui.update();
        ui
    }

    #[test]
    fn a_render_is_asked_only_while_render_reads_true() {
        assert_eq!(request(&ui("0", [1000.0, -500.0])), None);
        let r = request(&ui("1", [1000.0, -500.0])).unwrap();
        assert_eq!(r, Request { centre: [1000.0, -500.0, 300.0], half: 2048.0, offset: 10000.0 });
    }

    #[test]
    fn a_render_writes_its_centre_and_the_picture_name_for_the_file() {
        let mut u = ui("1", [1000.0, -500.0]);
        let r = request(&u).unwrap();
        rendered(&mut u, &r, "nvf:modern_minimap");
        assert_eq!(u.get_float(&format!("{ROOT}/_MapOriginX")), Some(1000.0));
        assert_eq!(u.get_float(&format!("{ROOT}/_MapOriginY")), Some(-500.0));
        assert_eq!(u.get_string(&format!("{ROOT}/MapRender/filename")).as_deref(), Some("nvf:modern_minimap"));
    }
}
