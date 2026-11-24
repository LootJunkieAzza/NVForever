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

[hypothesis: engineering policy] An unmeasured, invalid, fresh or restored
session returns zero. A valid sample requires a shown weather and finite progress
in 0..1. Invalid progress retires the fraction and classification together.
The existing two-argument setter keeps its outgoing/rain effects and invalidates
the measurement. A fraction-only host report cannot inherit an old classification.
No measurement is added to saves. Pending weather requests retain their existing
`GetIsCurrentWeather` precedence but do not masquerade as a measured sky.

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
