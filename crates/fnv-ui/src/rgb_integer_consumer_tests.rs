//! Authored RGB consumers: only nonnegative, exactly representable integer colours.

use crate::{MemSource, Ui};

fn source() -> MemSource {
    MemSource::default().with(
        "menus/rgb_fixture.xml",
        r#"<menu name="RgbFixture">
          <rect name="Option">
            <_input> 1125339 </_input>
            <_channelR><copy src="me()" trait="_input"/><shr>16</shr><land>255</land></_channelR>
            <_channelG><copy src="me()" trait="_input"/><shr>8</shr><land>255</land></_channelG>
            <_channelB><copy src="me()" trait="_input"/><land>255</land></_channelB>
            <image name="Swatch">
              <red><copy src="parent()" trait="_channelR"/></red>
              <green><copy src="parent()" trait="_channelG"/></green>
              <blue><copy src="parent()" trait="_channelB"/></blue>
            </image>
          </rect>
          <rect name="Picker">
            <_r>17</_r><_g>43</_g><_b>219</_b>
            <_output>
              <copy src="me()" trait="_r"/><shl>16</shl>
              <add><copy src="me()" trait="_g"/><shl>8</shl></add>
              <add><copy src="me()" trait="_b"/><shl>0</shl></add>
            </_output>
          </rect>
          <rect name="Neighbor">
            <width><copy>31</copy><add>12</add></width>
            <_bit><copy>5</copy><bt>2</bt></_bit>
            <_logical><copy>17</copy><and>43</and></_logical>
          </rect>
        </menu>"#,
    )
}

fn loaded() -> Ui {
    let mut ui = Ui::new();
    ui.load_menu(&source(), "menus/rgb_fixture.xml").unwrap();
    ui.update();
    ui
}

fn channels(ui: &Ui) -> [f64; 3] {
    ["red", "green", "blue"].map(|name| {
        ui.get_float(&format!("RgbFixture/Option/Swatch/{name}"))
            .unwrap()
    })
}

fn packed([r, g, b]: [u8; 3]) -> f64 {
    // The independent expected value is ordinary integer place-value arithmetic.
    f64::from(u32::from(r) * 65536 + u32::from(g) * 256 + u32::from(b))
}

#[test]
fn packed_option_value_reaches_distinct_swatch_channels_including_endpoints() {
    let mut ui = loaded();
    for rgb in [
        [17, 43, 219],
        [0, 0, 0],
        [255, 255, 255],
        [255, 0, 128],
        [0, 255, 1],
    ] {
        assert!(ui.set_float("RgbFixture/Option/_input", packed(rgb)));
        ui.update();
        assert_eq!(
            channels(&ui),
            rgb.map(f64::from),
            "packed option must reach the actual image channels for {rgb:?}"
        );
        assert_eq!(ui.get_float("RgbFixture/Neighbor/width"), Some(43.0));
    }
    assert!(
        ui.warnings.is_empty(),
        "supported RGB expressions must parse: {:?}",
        ui.warnings
    );
}

#[test]
fn picker_channels_reach_the_packed_value_without_logical_and_or_channel_aliasing() {
    let mut ui = loaded();
    for rgb in [
        [17, 43, 219],
        [0, 0, 0],
        [255, 255, 255],
        [255, 0, 128],
        [0, 255, 1],
    ] {
        for (channel, value) in ["_r", "_g", "_b"].into_iter().zip(rgb) {
            assert!(ui.set_float(&format!("RgbFixture/Picker/{channel}"), f64::from(value)));
        }
        ui.update();
        assert_eq!(
            ui.get_float("RgbFixture/Picker/_output"),
            Some(packed(rgb)),
            "picker must pack independently changed channels for {rgb:?}"
        );
    }
    assert!(ui.warnings.is_empty(), "{:?}", ui.warnings);
}

#[test]
fn selected_colour_roundtrip_live_changes_and_reentry_keep_the_color_consumers_consistent() {
    let mut ui = loaded();
    for rgb in [[29, 173, 82], [204, 5, 153], [0, 255, 255]] {
        for (channel, value) in ["_r", "_g", "_b"].into_iter().zip(rgb) {
            ui.set_float(&format!("RgbFixture/Picker/{channel}"), f64::from(value));
        }
        ui.update();
        let output = ui.get_float("RgbFixture/Picker/_output").unwrap();
        assert_eq!(
            output,
            packed(rgb),
            "edited value must reach packed output before applying it"
        );
        ui.set_float("RgbFixture/Option/_input", output);
        ui.update();
        assert_eq!(channels(&ui), rgb.map(f64::from));
        assert_eq!(ui.update(), 0, "an unchanged colour must settle");
        // A newly loaded tree receives the chosen value through the ordinary setter.
        let mut reopened = loaded();
        reopened.set_float("RgbFixture/Option/_input", output);
        reopened.update();
        assert_eq!(channels(&reopened), rgb.map(f64::from));
        assert_eq!(reopened.get_float("RgbFixture/Neighbor/width"), Some(43.0));
    }
}

#[test]
fn mapped_integer_domain_keeps_unsupported_values_and_retains_existing_operators() {
    let src = MemSource::default().with(
        "menus/integer_domain.xml",
        r#"<menu name="IntegerDomain">
      <_value>2147483648</_value><_argument>255</_argument>
      <_and><copy src="me()" trait="_value"/><LaNd src="me()" trait="_argument"/></_and>
      <_left><copy src="me()" trait="_value"/><SHL src="me()" trait="_argument"/></_left>
      <_right><copy src="me()" trait="_value"/><ShR src="me()" trait="_argument"/></_right>
      <_bit><copy>5</copy><bt>2</bt></_bit>
      <_logical><copy>17</copy><and>43</and></_logical>
      <width><copy>31</copy><add>12</add></width>
    </menu>"#,
    );
    let mut ui = Ui::new();
    ui.load_menu(&src, "menus/integer_domain.xml").unwrap();
    ui.update();
    // Out-of-i32-range operands keep the accumulator unchanged [design].
    for op in ["_and", "_left", "_right"] {
        assert_eq!(
            ui.get_float(&format!("IntegerDomain/{op}")),
            Some(2_147_483_648.0),
            "out-of-i32 value must keep accumulator for {op}"
        );
    }
    // In-range 32-bit bitwise mask is bitwise, not logical.
    ui.set_float("IntegerDomain/_value", 256.0);
    ui.set_float("IntegerDomain/_argument", 255.0);
    ui.update();
    assert_eq!(
        ui.get_float("IntegerDomain/_and"),
        Some(0.0),
        "mask is bitwise, not logical"
    );
    // Negative shift counts keep the accumulator unchanged [design], while `land` accepts signed i32.
    ui.set_float("IntegerDomain/_value", 13.0);
    ui.set_float("IntegerDomain/_argument", -1.0);
    ui.update();
    assert_eq!(ui.get_float("IntegerDomain/_left"), Some(13.0));
    assert_eq!(ui.get_float("IntegerDomain/_right"), Some(13.0));
    assert_eq!(ui.get_float("IntegerDomain/_and"), Some(13.0));
    // Out-of-i32-range operands (positive and negative) keep the accumulator unchanged [design].
    for (value, argument) in [
        (9_007_199_254_740_992.0, 8.0),
        (-2_147_483_649.0, 8.0),
        (13.0, 2_147_483_648.0),
    ] {
        ui.set_float("IntegerDomain/_value", value);
        ui.set_float("IntegerDomain/_argument", argument);
        ui.update();
        for op in ["_and", "_left", "_right"] {
            assert_eq!(
                ui.get_float(&format!("IntegerDomain/{op}")),
                Some(value),
                "out-of-i32 operand must keep accumulator: {op}({value}, {argument})"
            );
        }
    }
    for value in [f64::INFINITY, f64::NEG_INFINITY, f64::NAN] {
        ui.set_float("IntegerDomain/_value", value);
        ui.set_float("IntegerDomain/_argument", 8.0);
        ui.update();
        for op in ["_and", "_left", "_right"] {
            let got = ui.get_float(&format!("IntegerDomain/{op}")).unwrap();
            assert!(
                got == value || got.is_nan() && value.is_nan(),
                "unsupported nonfinite input is not coerced: {op}, {got}"
            );
        }
    }
    ui.set_string("IntegerDomain/_value", "unmapped text");
    ui.update();
    for op in ["_and", "_left", "_right"] {
        assert_eq!(
            ui.get(&format!("IntegerDomain/{op}")),
            Some(&crate::Value::Str("unmapped text".into()))
        );
    }
    assert_eq!(ui.get_float("IntegerDomain/_bit"), Some(1.0));
    assert_eq!(ui.get_float("IntegerDomain/_logical"), Some(1.0));
    assert_eq!(ui.get_float("IntegerDomain/width"), Some(43.0));
    assert!(ui.warnings.is_empty(), "{:?}", ui.warnings);
}

#[test]
fn observed_signed_32bit_rounding_and_shift_overflow_parity() {
    // [observed: `w4e_10_mcm_rgb_negative_frac_shift` (`tools/capture/nvse/catalogs/wave4e_mods.json`,
    // PR #3803 / #3802):
    // `PROBE|w4e_10_mcm_rgb_negative_frac_shift|rgb=255,128,64|land(neg=255,frac=4)|shl(neg=-20,frac=8,s32=0)|shr(neg=-4)`]
    // Verifies signed 32-bit two's-complement arithmetic, nearest-integer rounding,
    // and shift counts >= 32 returning 0 across literal and live-referenced traits.
    let src = MemSource::default().with(
        "menus/w4e10_bitwise.xml",
        r#"<menu name="BitwiseParity">
      <_packed>16744512</_packed>
      <_r><copy src="me()" trait="_packed"/><shr>16</shr><land>255</land></_r>
      <_g><copy src="me()" trait="_packed"/><shr>8</shr><land>255</land></_g>
      <_b><copy src="me()" trait="_packed"/><land>255</land></_b>
      <_land_neg><copy>-1</copy><LaNd>255</LaNd></_land_neg>
      <_land_frac><copy>3.7</copy><land>255</land></_land_frac>
      <_land_neg_frac><copy>-3.7</copy><land>255</land></_land_neg_frac>
      <_shl_neg><copy>-5</copy><SHL>2</SHL></_shl_neg>
      <_shl_frac><copy>3.7</copy><shl>1</shl></_shl_frac>
      <_shl_32><copy>1</copy><shl>32</shl></_shl_32>
      <_shl_31><copy>1</copy><shl>31</shl></_shl_31>
      <_shr_neg><copy>-8</copy><ShR>1</ShR></_shr_neg>
      <_shr_32><copy>1</copy><shr>32</shr></_shr_32>
      <_v>3.7</_v><_a>1.6</_a>
      <_live_shl><copy src="me()" trait="_v"/><shl src="me()" trait="_a"/></_live_shl>
      <_live_shr><copy src="me()" trait="_v"/><shr src="me()" trait="_a"/></_live_shr>
      <_live_land><copy src="me()" trait="_v"/><land src="me()" trait="_a"/></_live_land>
    </menu>"#,
    );
    let mut ui = Ui::new();
    ui.load_menu(&src, "menus/w4e10_bitwise.xml").unwrap();
    ui.update();

    // RGB unpack of 0xFF8040 = 16744512 -> (255, 128, 64)
    assert_eq!(ui.get_float("BitwiseParity/_r"), Some(255.0));
    assert_eq!(ui.get_float("BitwiseParity/_g"), Some(128.0));
    assert_eq!(ui.get_float("BitwiseParity/_b"), Some(64.0));

    // Exact Wave 4E w4e_10 observations:
    assert_eq!(ui.get_float("BitwiseParity/_land_neg"), Some(255.0));
    assert_eq!(ui.get_float("BitwiseParity/_land_frac"), Some(4.0));
    assert_eq!(ui.get_float("BitwiseParity/_land_neg_frac"), Some(252.0));
    assert_eq!(ui.get_float("BitwiseParity/_shl_neg"), Some(-20.0));
    assert_eq!(ui.get_float("BitwiseParity/_shl_frac"), Some(8.0));
    assert_eq!(ui.get_float("BitwiseParity/_shl_32"), Some(0.0));
    assert_eq!(ui.get_float("BitwiseParity/_shl_31"), Some(f64::from(i32::MIN)));
    assert_eq!(ui.get_float("BitwiseParity/_shr_neg"), Some(-4.0));
    assert_eq!(ui.get_float("BitwiseParity/_shr_32"), Some(0.0));

    // Fractional rounding applies to both operands (3.7 -> 4, 1.6 -> 2):
    assert_eq!(ui.get_float("BitwiseParity/_live_shl"), Some(16.0));
    assert_eq!(ui.get_float("BitwiseParity/_live_shr"), Some(1.0));
    assert_eq!(ui.get_float("BitwiseParity/_live_land"), Some(0.0));

    // Live updates recompute through the same signed 32-bit path:
    assert!(ui.set_float("BitwiseParity/_v", -13.0));
    assert!(ui.set_float("BitwiseParity/_a", 2.0));
    ui.update();
    assert_eq!(ui.get_float("BitwiseParity/_live_shl"), Some(-52.0));
    assert_eq!(ui.get_float("BitwiseParity/_live_shr"), Some(-4.0));
    assert_eq!(ui.get_float("BitwiseParity/_live_land"), Some(2.0));

    // Shift >= 32 returns 0 even for large counts (e.g. 54):
    assert!(ui.set_float("BitwiseParity/_a", 54.0));
    ui.update();
    assert_eq!(ui.get_float("BitwiseParity/_live_shl"), Some(0.0));
    assert_eq!(ui.get_float("BitwiseParity/_live_shr"), Some(0.0));
    assert!(ui.warnings.is_empty(), "{:?}", ui.warnings);
}
