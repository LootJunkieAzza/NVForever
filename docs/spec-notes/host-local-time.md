# Host-local time with Jiff

[documented: [GetUserTime](https://geckwiki.com/index.php?title=GetUserTime),
[GetTimeStamp](https://geckwiki.com/index.php?title=GetTimeStamp)] GetUserTime and
GetTimeStamp give the computer's local time. GetFileTime and GetFileTimeSTR use the
same local time [hypothesis: their pages show no time zone, see
[ext-sup5](ext-sup5.md)].

[design] All four read the host zone through Jiff's system timezone
([`TimeZone`](https://docs.rs/jiff/1.0.0/jiff/tz/struct.TimeZone.html),
[`Timestamp`](https://docs.rs/jiff/1.0.0/jiff/struct.Timestamp.html)). If discovery
fails, they use UTC and emit one warning per process, in NVForever's own words.

[design] An actual file timestamp outside Jiff's representable range is treated as
unavailable metadata: GetFileTime returns an empty list and GetFileTimeSTR returns
an empty string, as for a missing file ([ext-sup5](ext-sup5.md), itself
[hypothesis]). Timestamp conversion happens before timezone lookup.

[hypothesis] The Windows-to-IANA mapping can differ from Windows historical or
per-machine rules, including when automatic daylight saving adjustment is off.
Original-game and installed-mod parity are not established.
