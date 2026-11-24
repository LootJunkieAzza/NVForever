# NAT traversal: STUN, PCP, NAT-PMP and the checks

Status: 2026-11-24. The formats `crates/nvf-nat` implements, for the co-op networking design
(#3795 sections 7.1 and 7.2). All three protocols are public IETF standards; nothing here comes
from the game. Tags as the charter asks.

## STUN Binding (RFC 8489)

- A message is a 20-byte header and attributes [documented: RFC 8489 section 5]:
  - Type (2 bytes): Binding request 0x0001, success response 0x0101. The top two bits are 0.
  - Length (2 bytes): bytes after the header, a multiple of 4.
  - Magic cookie 0x2112A442 (4 bytes).
  - Transaction id (12 bytes), random, copied into the answer.
- An attribute is type (2), length of the value before padding (2), the value, then zero
  padding to a multiple of 4 [documented: section 14].
- Attribute types read or written [documented: section 18.3]: MAPPED-ADDRESS 0x0001, ERROR-CODE
  0x0009, XOR-MAPPED-ADDRESS 0x0020, SOFTWARE 0x8022, FINGERPRINT 0x8028.
- XOR-MAPPED-ADDRESS [documented: section 14.2]:
  - A zero byte, the family (1 for IPv4, 2 for IPv6), the port, then the address.
  - The port is XORed with 0x2112.
  - An IPv4 address is XORed with the cookie. An IPv6 address is XORed with the cookie followed
    by the transaction id.
  - RFC 5769 sections 2.2 and 2.3 give worked values, used as the unit tests [documented].
- FINGERPRINT is CRC-32 (gzip's) over everything before it, XOR 0x5354554E. The header length
  already counts the FINGERPRINT attribute, and the attribute comes last [documented: section 14.7].
- Resends: 500 ms, doubling, the same transaction id, 7 sends at most [documented: section
  6.2.1]. NVForever stops at 3 s [design].
- Two servers that see the same public address mean one public port for every destination.
  Different ports mean a new port per destination (a "symmetric" NAT) [design, the method of
  RFC 4787's terms].

## PCP MAP (RFC 6887)

- Requests and answers go to the gateway's UDP port 5351 [documented: section 19.1].
- Request: version 2, opcode 1 (MAP), 2 reserved bytes, lifetime (4), the client's own
  address as 16 bytes, then the MAP data [documented: sections 7.1 and 11.1]:
  - nonce (12, random);
  - protocol (1, 17 for UDP) and 3 reserved bytes;
  - internal port (2);
  - suggested external port (2) and address (16).
- IPv4 addresses are written as IPv4-mapped IPv6 (`::ffff:a.b.c.d`) [documented: section 5].
- Answer: the same layout. Byte 1 has its top bit set, byte 3 is the result code, bytes 4 to
  7 the lifetime granted, 8 to 11 the epoch. Bytes 42 to 59 hold the external port and address
  given [documented: sections 7.2 and 11.1].
- The client checks the nonce. It ignores answers under 24 bytes, over 1100 bytes or not a
  multiple of 4 [documented: section 8.3].
- Result 1 means an unsupported version [documented: section 7.4].
- Renew from half the lifetime [documented: section 11.2.1]. Lifetime 0 removes the mapping.
- RFC 6887 asks for 3 s before the first resend [documented: section 8.1.1]. NVForever resends
  at 250 ms, doubling, and gives up at 2 s, so a router without PCP does not hold a join up
  [design].

## NAT-PMP (RFC 6886)

- Version 0, on the same port 5351. An answer's opcode is 128 plus the request's
  [documented: sections 3 and 3.1].
- External address request: `00 00`. Answer (12 bytes): version, 128, result (2), epoch (4),
  IPv4 address (4) [documented: section 3.2].
- UDP map request (12 bytes): version, opcode 1, 2 reserved bytes, internal port (2), suggested
  external port (2), lifetime (4). Answer (16 bytes): version, 129, result (2), epoch (4),
  internal port (2), external port (2), lifetime (4) [documented: section 3.3]. Lifetime 0
  removes the mapping [documented: section 3.4].
- An unsupported request gets a version-0 answer with result 1 [documented: section 3.5]. That
  is how a client that tried PCP first finds a NAT-PMP-only router.
- Only answers from the gateway count [documented: section 3.2].

## The checks (ICE, cut down)

[design: #3795 section 7.2; the triggered-check and peer-reflexive rules follow RFC 8445's
ideas, not its message format]

- Every local socket is paired with every remote candidate of its family.
- Pairs rank IPv6 direct, then LAN, then a mapped port or a public host address, then a STUN-seen
  or peer-learnt address.
- Both sides send one probe every 20 ms, round-robin over the unanswered pairs, for 10 s.
  The probes are Noise handshake messages, sent by the caller.
- A probe that arrives puts its pair at the front of the queue (a triggered check).
- An answer from an address that is not the one probed teaches a new, peer-reflexive
  candidate. That is how a symmetric NAT's per-destination port is found.
- The guest chooses the pair. It picks the best class that answered, at once if no better class
  is left to try, otherwise after 200 ms. Then it tells the host over that pair.
- On the chosen pair, a keepalive goes every 1 s while nothing else is sent. After 4 s without
  hearing from the other side, the path counts as lost.

## Simulated NATs

`natsim` models six kinds by mapping and filtering: open, full cone, restricted,
port-restricted, symmetric and blocked. It does no hairpinning. Its matrix test expects every
pair to connect except these [design: the textbook result]:

- anything with a blocked side;
- symmetric against symmetric;
- symmetric against port-restricted.

Two machines on one LAN connect over the LAN behind any kind.
