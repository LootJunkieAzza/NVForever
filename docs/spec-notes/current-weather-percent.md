# Script weather measurements

This source proposal joins `GetCurrentWeatherPercent`, `IsSnowing`, `IsPleasant`
and `IsCloudy` at the existing sky-to-script report. It adds no weather timer,
selection policy, catalogue entry, native version claim or serialized state.

## Contracts and uncertainty

[documented] The GECK references describe
[GetCurrentWeatherPercent](https://geckwiki.com/index.php/GetCurrentWeatherPercent)
as transition progress in 0..1 and provide the alias `getweatherpct`.
[IsPleasant](https://geckwiki.com/index.php?title=IsPleasant) describes destination
Pleasant classification with intermediate transition values.
[IsSnowing](https://geckwiki.com/index.php?title=IsSnowing) describes a settled
Snow result and explicitly distinguishes Fallout's binary result from fractional
Oblivion behavior. Its separate example appears inconsistent with that settled
description; these sources are not native endpoint or timing measurements.

`IsCloudy` uses the corresponding destination-Cloudy fraction described by the
repository catalogue and corroborated by the GECK Snow/Rain pages' distinction
between binary and fractional weather queries. The linked
[Construction Set reference](https://cs.uesp.net/wiki/IsCloudy) is for Oblivion,
not an FNV execution result. The exact FNV `IsCloudy` page could not be retrieved.
The selected destination-class rule is an explicit portable policy where that
provenance stops; native outgoing/reversal behavior and endpoint timing remain
unmeasured. No outgoing `1-fade` contribution is inferred.

The existing WTHR DATA importer supplies classification bits 1 Pleasant, 2 Cloudy,
4 Rainy and 8 Snow. `SkyState` already owns destination weather, previous weather,
and destination progress. Those are repository source facts. The batch reads
them without changing their producer, fade scheduling, override/release,
fast-travel hold or `keep_weather_once` behavior. Existing `IsRaining` is unchanged.
Unrelated classification bits do not manufacture a known class. Existing JIP
`SetWeatherTraitNumeric` session overrides remain stored and visible to that
trait getter; fog traits `15..=20` propagate live to exterior `sky.rs` fog
(docs/spec-notes/world.md §6), while non-fog weather traits (`0..=14`) remain
stored-only for this batch.

## Report and lifetime

The actual `script_weather` report remains before `script_sky` and
`sky_follows_game`. It keeps the existing shown/rain update guard, then publishes
one finite sample on every bridge pass even when the shown ID and rain flag stay
the same. Readers consume the last report; they do not predict the next clock
update. Classification comes from the destination WTHR; the outgoing weather
does not supply a second fractional contribution.

[observed: Fallout: New Vegas 1.4.0.525 + xNVSE 6.4.9 + JIP LN 57.30, 2026-11-24,
GSDocMitchellHouse (`0x00103DF9`) and Goodsprings (`0x000CDF51`), 10 official base
masters, isolated staging copy (read-only), committed probe `w4g_04_weather_cold_start`
in `tools/capture/nvse/catalogs/wave4g_f2_extenders.json` (companion catalog PR #3803);
no DLL or source consulted] Immediately on cold start in an interior before ever
visiting an exterior, the original game already has settled `NVDefaultWeather`
(`0x00064609`, Pleasant) active with `GetCurrentWeatherPercent = 1.0`,
`IsPleasant = 1.0`, `IsSnowing = 0`, `IsRaining = 0`, `IsCloudy = 0.0`, and
`GetIsCurrentWeather(cur) = 1`; moving to Goodsprings exterior resolves
`NVGoodspringsWeather` (`0x001237D7`) with the same settled values. Verbatim probe
output:
- `PROBE|w4g_04_weather_cold_start|cold(pct=1.0000,snow=0,rain=0,pls=1.0000,cld=0.0000,cur=00064609,isDef=0,isCur=1)`
- `PROBE|w4g_04_weather_exterior|moved=1`
- `PROBE|w4g_04_weather_exterior|ext(pct=1.0000,snow=0,rain=0,pls=1.0000,cld=0.0000,cur=001237D7,isCur=1)`

[design] When the load order defines vanilla `NVDefaultWeather` (`0x00064609`, WTHR),
`Game::new` initializes `WeatherState` with `NVDefaultWeather` shown and settled at
`1.0` using its authored WTHR `DATA` classification byte (defaulting to Pleasant `1` if
`DATA` is omitted). Synthetic load orders that do not define `0x00064609`, invalidated
samples, legacy two-argument `set_shown_weather` writes, and restored snapshots before
the sky bridge reports remain unmeasured and return zero. A valid sample requires a
shown weather and finite progress in `0..=1`. Invalid progress retires the fraction and
classification together. A fraction-only host report cannot inherit an old
classification. No measurement is added to saves. Pending weather requests retain their
existing `GetIsCurrentWeather` precedence but do not masquerade as a measured sky.

## Authored controls and qualification

The seven original percent controls are retained byte-for-byte as a prefix.
Eight additional shared-App controls use authored WTHR records through the real
importer, real SkyState fade/clock methods, the production reporting helper and
numeric, compiled-source and raw CTDA consumers. They cover classification
freshness, stable/instant/fractional transitions, outgoing/pending neighbors,
cold/restore/legacy state, invalid measurements and report-before-clock cadence.
The test builder is shared with the existing lighting fixtures; it is not loaded
twice under different module names.

The controlled baseline neutrally extracts the original report body and exposes
existing producer/test helpers, with the identical fifteen fixtures. It is not
pristine main and calls no new Game API. Execution results are qualified per exact source and dependency context;
authored coverage alone does not qualify execution. Full ScriptState scheduling,
rendering, native parity and execution of original installed mods remain outside
this source-only evidence.

## Wave 6D-10 Live Clean-Room Weather, Climate & Sky Baselines

- **Interior `ForceWeather` / `SetWeather` / `ReleaseWeatherOverride` & wind speed:** indoors `GetCurrentWeatherPercent` reads `1.00`, `ForceWeather` immediately updates `IsCloudy`/`IsPleasant` (`cl=1.00, pl=0.00`), `ReleaseWeatherOverride` restores default weather on subsequent tick, and `GetWindSpeed` stays `0.000` indoors even with `SetWindSpeedMult 2.0` or high-wind weather [observed: Fallout: New Vegas 1.4.0.525 (040020D0) + xNVSE 6.4.9 + JIP LN NVSE 57.30 (5730), 2026-11-25, `GSDocMitchellHouse` (`0x00103DF9`), 10 official base masters, read-only console probe `w6d10_13_force_set_release_weather_and_windspeed_indoors` (`tools/capture/nvse/catalogs/wave6d10_world_weather_water_map.json`): `PROBE|w6d10_13_force_set_release_weather_and_windspeed_indoors|b0(00064609,p=1.00,w=0.000,pl=1,cl=0)|fCld(1,1,p=1.00,cl=1.00,pl=0.00,w=0.000,m2=0.000)|setClr(0,0,p=1.00)|dustW=0.000|post(eqD=1,p=1.00,w=0.000,rn=0,sn=0)`; zero DLL, PDB, or third-party source consulted].
- **`GetWeatherTraitNumeric` / `SetWeatherTraitNumeric`:** verified cloud speed, wind speed (`0.1961`), transition delta (`0.255`), glare (`0.212`), lightning frequency (`1.000`), lightning RGB (`200150100`), fog distances (`200000`, `0.60`), classification rain/snow bit toggles (`r4=1/0`, `s8=0/1`), and `u8`-backed wind speed quantization/wrapping (`0.5 -> 0.4980`, `2.0 -> 1.000`, `-0.5 -> 0.506`) [observed: Fallout: New Vegas 1.4.0.525 (040020D0) + xNVSE 6.4.9 + JIP LN NVSE 57.30 (5730), 2026-11-25, `GSDocMitchellHouse` (`0x00103DF9`), 10 official base masters, read-only console probe `w6d10_14_weather_traits_cloud_wind_glare_class_and_rain_flag` (`tools/capture/nvse/catalogs/wave6d10_world_weather_water_map.json`): `PROBE|w6d10_14_weather_traits_cloud_wind_glare_class_and_rain_flag|base(c0=0.0204,w=0.1961,td=0.255,gl=0.212,lf=1.000)|cls=1,lrgb=0->200150100,f16=200000,f19=0.60|w4(0.5=0.4980,hi=1.000,lo=0.506)|td=0.1230|cls(r4=1/0,s8=0/1)`; zero DLL, PDB, or third-party source consulted].
- **Weather precipitation model, cloud textures & ImageSpace modifiers:** `SetWeatherPrecipitationModel`, `SetWeatherTexture` (`0..=3`, negative index returns `0`), and `GetWeatherImageSpaceMod` (`0..=5` valid, out-of-range returns `0`) round-trip cleanly [observed: Fallout: New Vegas 1.4.0.525 (040020D0) + xNVSE 6.4.9 + JIP LN NVSE 57.30 (5730), 2026-11-25, `GSDocMitchellHouse` (`0x00103DF9`), 10 official base masters, read-only console probe `w6d10_16_weather_precip_model_textures_and_imad_times` (`tools/capture/nvse/catalogs/wave6d10_world_weather_water_map.json`): `PROBE|w6d10_16_weather_precip_model_textures_and_imad_times|pm(len0=0,set=[Effects\w6d10_rain.nif])|tx(l0=13,l3=20,neg=0,set=[Sky\w6d10_cloud.dds])|im(0015E247,001735E6,000CEE19,0015E247,0015E247,00000014|oob=1,1)`; zero DLL, PDB, or third-party source consulted].
- **Climate traits & `IsNight` tick update:** `GetClimateTraitNumeric` returns sunrise/sunset/volatility/moon-length/phase-length (`360, 480, 1080, 1200, v=0, m=2, p=3`), out-of-range indices return `0`, and `IsNight` updates after a game tick when `GameHour` crosses into night (`same2=0/0`, `post2=1/1`) [observed: Fallout: New Vegas 1.4.0.525 (040020D0) + xNVSE 6.4.9 + ShowOffNVSE Plugin 184 + JIP LN NVSE 57.30 (5730), 2026-11-25, `GSDocMitchellHouse` (`0x00103DF9`), 10 official base masters, read-only console probe `w6d10_18_climate_traits_sun_volatility_and_isnight_edges` (`tools/capture/nvse/catalogs/wave6d10_world_weather_water_map.json`): `PROBE|w6d10_18_climate_traits_sun_volatility_and_isnight_edges|clm(360,480,1080,1200,v=0,m=2,p=3)|oob(0,0)|trq(m=0,p=3)|same2=0/0|post2=1/1`; zero DLL, PDB, or third-party source consulted].
- **`GetCurrentSkyColor`, `TriggerLightningFX` & wind direction:** `GetCurrentSkyColor` returns 3-element RGB arrays for types `1..=4` (empty/uninitialized for negative index), `TriggerLightningFX` returns `0` indoors, and `GetWindDirection` / `SetWindDirection` store raw values without clamping (`0=-57.30`, `n=-90.0`, `ov=400.0`) [observed: Fallout: New Vegas 1.4.0.525 (040020D0) + xNVSE 6.4.9 + JIP LN NVSE 57.30 (5730) + JohnnyGuitarNVSE 528, 2026-11-25, `GSDocMitchellHouse` (`0x00103DF9`), 10 official base masters, read-only console probe `w6d10_21_current_sky_color_and_trigger_lightning_indoors` (`tools/capture/nvse/catalogs/wave6d10_world_weather_water_map.json`): `PROBE|w6d10_21_current_sky_color_and_trigger_lightning_indoors|sky(t1=1:0.29,0.27,0.19|t2=1:1.00)|t3=1:0.12|t4=1:0.13|neg=0:111|mut1=1:0.29,0.27,0.19|lfx=0|wd(0=-57.30,n=-90.0,ov=400.0)`; zero DLL, PDB, or third-party source consulted].
