# JIP LN `IsInKillCam`

[documented] Per the GECK wiki (`https://geckwiki.com/index.php?title=IsInKillCam`),
`IsInKillCam` is a JIP LN NVSE function taking no arguments and returning `1` if the game
is currently displaying a kill camera, or `0` otherwise.

[design] NVForever already implements kill cameras in `nvf_play::kill_cam::KillCam`, which
runs two kinds of camera shows: kill cameras (`Kind::Kill`, for VATS last-mark kills and
non-VATS kill cameras) and classic VATS attack cameras (`Kind::Attack(_)`). Each frame,
`nvf_play::script_input::tell_player_state` (scheduled after `PlaySet::Activation` and
before `PlaySet::Scripts`) publishes `KillCam::kill_playing()` (true only for `Kind::Kill`,
excluding non-kill classic VATS attack cameras, and `false` when `KillCam` is absent) into
`fnv_game::nvse_jip2::PlayerTold::kill_cam`, which `IsInKillCam` returns as `1.0` or `0.0`.

[hypothesis] Native edge cases such as whether non-VATS cinematic kill cameras, VATS
kill-shot transition frames, or CTDA condition evaluation differ from `Kind::Kill` have not
been measured in an original-game capture.
