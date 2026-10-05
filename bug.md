# CDJ-3000 hard crash while browsing RBX LINK

Status: open, physical-device incident, root cause not yet proven  
Incident date: 2026-10-04 PDT / 2026-10-05 UTC  
Primary incident: 19:07:10 PDT (`2026-10-05T02:07:10Z`)  
Investigation date: 2026-10-04 PDT  
Repository branch at investigation: `dev`  
Repository HEAD at investigation: `29dc378`  

This document deliberately distinguishes observations from inference. `[OBS]`
means the app log or the user directly established it, `[INFER]` means the
evidence strongly suggests it, and `[UNKNOWN]` means the available evidence
cannot settle it.

## User report and preceding symptoms

- [OBS] While connected to RBX LINK, the physical CDJ-3000 "crashed hard."
- [OBS] Before the crash, playlist preview waveforms were intermittent: on
  different occasions a few appeared, none appeared, or all appeared.
- [OBS] LINK cue previewing sometimes ran fast and sometimes slow.
- [OBS] Loading songs felt slower than before.
- [OBS] Rekordbox/RBX was configured to show classic musical keys, but the CDJ
  browse list still showed alphanumeric/Camelot keys such as `1A`, `1B`, and
  `3A` in its KEY column. Track titles themselves contained values such as
  `4A - Ab - 135`, so title text and the CDJ's KEY column were not the same
  field.

The waveform, cue-preview, and load symptoms matter because the crash was
preceded by an unusually dense burst of artwork and analysis requests from the
same CDJ.

## Devices and network observed by RBX

The incident session used:

- RBX/rekordbox identity: device 17 at `192.168.1.14`, interface `en11`
- Crashed CDJ-3000: device 2 at `192.168.1.35`, MAC
  `24:97:ed:23:10:72`
- Other CDJ-3000: device 1 at `192.168.1.170`, MAC
  `24:97:ed:1e:9b:5d`
- Mixer: device 33, `DJM-V5`, at `192.168.1.66`
- `ProLink-Connect`: device 4 at `192.168.1.211`
- Loaded library: 38,740 tracks and 680 playlists
- Database query port: `192.168.1.14:12523`
- Database session port for this run: `192.168.1.14:50278`
- NFS: `192.168.1.14:2049`

The app's source log is:

```text
~/Library/Application Support/rbxport/logs/rbxport.2026-10-05.log
```

The filename is UTC-dated. Its `2026-10-05T02:...Z` entries occurred on
October 4 in PDT.

## Session establishment

RBX started LINK at 18:53:31 PDT. The affected CDJ opened database session
`192.168.1.35:51596` five seconds later.

```text
2026-10-05T01:53:31.109353Z  INFO rbxport_lib::commands: LINK starting interface="auto"
2026-10-05T01:53:31.111170Z DEBUG rbxport_lib::link: interface chosen: the one that reaches a device already heard interface=en11 peer=192.168.1.66
2026-10-05T01:53:31.111183Z  INFO rbxport_lib::link: LINK running on an interface interface=en11 address=192.168.1.14
2026-10-05T01:53:31.121866Z DEBUG rbl_dbserver::net: database server bound query=192.168.1.14:12523 database=192.168.1.14:50278
2026-10-05T01:53:31.177704Z  INFO rbl_link: link export started interface=en11 address=192.168.1.14 database=192.168.1.14:50278 nfs=192.168.1.14:2049
2026-10-05T01:53:31.945100Z  INFO rbl_link::beacon: new device on the link number=2 name=CDJ-3000 kind=Cdj ip=192.168.1.35 mac=24 97 ed 23 10 72
2026-10-05T01:53:36.294501Z DEBUG rbl_link::beacon: media query about our slot; answering from=192.168.1.35 slot=4 tracks=38740 playlists=680
2026-10-05T01:53:36.296664Z DEBUG rbl_link::beacon: link handshake; answering player=192.168.1.35
2026-10-05T01:53:36.297603Z TRACE rbl_dbserver::net: connection accepted peer=192.168.1.35:47540
2026-10-05T01:53:36.297679Z DEBUG rbl_dbserver::net: port query answered peer=192.168.1.35:47540 port=50278
2026-10-05T01:53:36.298824Z TRACE rbl_dbserver::net: connection accepted peer=192.168.1.35:51596
2026-10-05T01:53:36.298872Z DEBUG rbl_dbserver::net: player connected to the database server peer=192.168.1.35:51596
2026-10-05T01:53:36.298973Z DEBUG rbl_dbserver::net: database greeting exchanged peer=192.168.1.35:51596
```

## Traffic storm immediately before the crash

[OBS] The affected player generated the following database-request rates. These
are counts of `database request` log records for `192.168.1.35`, grouped by
UTC second:

| UTC second | PDT | Requests |
|---|---:|---:|
| `02:06:42` | 19:06:42 | 57 |
| `02:06:43` | 19:06:43 | 72 |
| `02:06:54` | 19:06:54 | 40 |
| `02:06:55` | 19:06:55 | 94 |
| `02:07:00` | 19:07:00 | 114 |
| `02:07:01` | 19:07:01 | 92 |
| `02:07:02` | 19:07:02 | 29 |
| `02:07:03` | 19:07:03 | 34 |

For comparison, the highest observed second from the other CDJ at
`192.168.1.170` in this log was 32 requests. That comparison is not a
controlled test because the user actions may have differed.

[OBS] Between `02:07:00.000Z` and `02:07:03.999Z`, RBX processed 269 database
requests and sent:

- 903 database messages
- 1,778,418 bytes in those messages
- 99 artwork replies
- 99 `.2EX` analysis-tag replies

[OBS] Across the complete `02:07:00.000Z` through `02:07:10.547316Z` crash
window, RBX processed 291 requests and sent 1,077 messages totaling 1,802,640
bytes.

The requests were issued by the CDJ. RBX did not push this artwork or analysis
unsolicited. RBX answered them very quickly, which allowed the player's request
loop to run at more than 100 requests per second.

Representative raw lines from the start of the densest burst:

```text
2026-10-05T02:07:00.009645Z TRACE rbl_dbserver::net: database request peer=192.168.1.35:51596 tx=404 kind=playlist menu args=[33620993, 0, 200496, 1]
2026-10-05T02:07:00.013116Z TRACE rbl_dbserver::net: database request peer=192.168.1.35:51596 tx=405 kind=render args=[33620993, 0, 25, 0, 58, 12, 1, 0]
2026-10-05T02:07:00.024428Z TRACE rbl_dbserver::net: database request peer=192.168.1.35:51596 tx=406 kind=playlist menu args=[33686529, 12, 478981765, 0]
2026-10-05T02:07:00.030070Z TRACE rbl_dbserver::net: database request peer=192.168.1.35:51596 tx=407 kind=render args=[33686529, 0, 25, 0, 47, 12, 1, 2]
2026-10-05T02:07:00.045172Z TRACE rbl_dbserver::net: database request peer=192.168.1.35:51596 tx=408 kind=artwork args=[34079745, 85261874, 1]
2026-10-05T02:07:00.045745Z TRACE rbl_dbserver::net: database request peer=192.168.1.35:51596 tx=409 kind=anlz tag (2EX) args=[34079745, 85261874, 911628112, 5784882]
2026-10-05T02:07:00.054376Z TRACE rbl_dbserver::net: database request peer=192.168.1.35:51596 tx=410 kind=artwork args=[34079745, 210097002, 1]
2026-10-05T02:07:00.066396Z TRACE rbl_dbserver::net: database request peer=192.168.1.35:51596 tx=411 kind=anlz tag (2EX) args=[34079745, 210097002, 911628112, 5784882]
2026-10-05T02:07:00.071892Z TRACE rbl_dbserver::net: database request peer=192.168.1.35:51596 tx=412 kind=artwork args=[34079745, 203547213, 1]
2026-10-05T02:07:00.079499Z TRACE rbl_dbserver::net: database request peer=192.168.1.35:51596 tx=413 kind=anlz tag (2EX) args=[34079745, 203547213, 911628112, 5784882]
2026-10-05T02:07:00.084339Z TRACE rbl_dbserver::net: database request peer=192.168.1.35:51596 tx=414 kind=artwork args=[34079745, 191912734, 1]
2026-10-05T02:07:00.087408Z TRACE rbl_dbserver::net: database request peer=192.168.1.35:51596 tx=415 kind=artwork args=[34079745, 114047295, 1]
2026-10-05T02:07:00.095794Z TRACE rbl_dbserver::net: database request peer=192.168.1.35:51596 tx=416 kind=anlz tag (2EX) args=[34079745, 191912734, 911628112, 5784882]
2026-10-05T02:07:00.096293Z TRACE rbl_dbserver::net: database request peer=192.168.1.35:51596 tx=417 kind=artwork args=[34079745, 98337872, 1]
```

All inspected `.2EX` replies in this burst were successful replies carrying a
3,624-byte blob, encoded as a 3,674-byte database message. Example:

```text
2026-10-05T02:07:00.045991Z TRACE rbl_dbserver::net: database message sent peer=192.168.1.35:51596 tx=409 kind=anlz tag reply args=[11524, 0, 3624, <3624 bytes>, 1] len=3674
```

Artwork replies varied with the source image. Most succeeded; at least one
returned status 50 with no bytes. Examples observed in the burst ranged from
2,373 bytes to more than 26 KB:

```text
kind=artwork reply args=[8195, 0, 2373, <2373 bytes>]
kind=artwork reply args=[8195, 0, 16103, <16103 bytes>]
kind=artwork reply args=[8195, 0, 26218, <26218 bytes>]
kind=artwork reply args=[8195, 50, 0, <0 bytes>]
```

## Last track interaction and exact crash sequence

The final track involved was content ID `267359040`:

- Title: `La Révolution (Extended Mix)`; the logged title uses a decomposed
  combining accent.
- Artist: `MORTEN & David Guetta`
- Label shown by the metadata reply: `Delivered By Inflyte`
- BPM: `13200` (132.00 BPM)
- Key text returned by RBX: `Em`
- Source path:
  `/Volumes/SD/RB/_2026-08-dance/MORTEN & David Guetta - La Révolution (Extended Mix).mp3`

The player had already requested and successfully received the track's artwork
and `.2EX` analysis several seconds before the crash:

```text
2026-10-05T02:07:02.581108Z TRACE rbl_dbserver::net: database request peer=192.168.1.35:51596 tx=622 kind=artwork args=[34079745, 267359040, 1]
2026-10-05T02:07:02.581489Z TRACE rbl_dbserver::net: database reply peer=192.168.1.35:51596 tx=622 replies=1
2026-10-05T02:07:02.581513Z TRACE rbl_dbserver::net: database message sent peer=192.168.1.35:51596 tx=622 kind=artwork reply args=[8195, 0, 4659, <4659 bytes>] len=4703
2026-10-05T02:07:02.590368Z TRACE rbl_dbserver::net: database request peer=192.168.1.35:51596 tx=624 kind=anlz tag (2EX) args=[34079745, 267359040, 911628112, 5784882]
2026-10-05T02:07:02.591985Z TRACE rbl_dbserver::net: database reply peer=192.168.1.35:51596 tx=624 replies=1
2026-10-05T02:07:02.592025Z TRACE rbl_dbserver::net: database message sent peer=192.168.1.35:51596 tx=624 kind=anlz tag reply args=[11524, 0, 3624, <3624 bytes>, 1] len=3674
```

Immediately before the socket closed, the player requested metadata, track
information, and the plain cue list for that track. These are the complete raw
log lines from the final request through the reboot detection:

```text
2026-10-05T02:07:09.740391Z TRACE rbl_dbserver::net: database request peer=192.168.1.35:51596 tx=687 kind=metadata args=[33686529, 267359040]
2026-10-05T02:07:09.740648Z TRACE rbl_dbserver::net: database reply peer=192.168.1.35:51596 tx=687 replies=1
2026-10-05T02:07:09.740661Z TRACE rbl_dbserver::net: database message sent peer=192.168.1.35:51596 tx=687 kind=menu header args=[8194, 16] len=32
2026-10-05T02:07:09.745050Z TRACE rbl_dbserver::net: database bytes received peer=192.168.1.35:51596 len=68
2026-10-05T02:07:09.745067Z TRACE rbl_dbserver::net: database request peer=192.168.1.35:51596 tx=688 kind=render args=[33686529, 0, 16, 0, 16, 12, 1, 0]
2026-10-05T02:07:09.745097Z TRACE rbl_dbserver::net: database reply peer=192.168.1.35:51596 tx=688 replies=18
2026-10-05T02:07:09.745107Z TRACE rbl_dbserver::net: database message sent peer=192.168.1.35:51596 tx=688 kind=render header args=[1, 0] len=32
2026-10-05T02:07:09.745149Z TRACE rbl_dbserver::net: database message sent peer=192.168.1.35:51596 tx=688 kind=menu item args=[267359040, 267359040, 60, "La Re\u{301}volution (Extended Mix)", 42, "Delivered By Inflyte", 8964, 0, 267359040, 0, 256, 17, 1204703889, 6, "Em", 13200] len=224
2026-10-05T02:07:09.745191Z TRACE rbl_dbserver::net: database message sent peer=192.168.1.35:51596 tx=688 kind=menu item args=[1, 1834770944, 44, "MORTEN & David Guetta", 2, "", 7, 0, 0, 0, 0, 0, 0, 2, "", 0] len=164
2026-10-05T02:07:09.745231Z TRACE rbl_dbserver::net: database message sent peer=192.168.1.35:51596 tx=688 kind=menu item args=[1, 1215825304, 76, "MORTEN & David Guetta - La Révolution", 2, "", 2, 0, 0, 0, 0, 0, 0, 2, "", 0] len=196
2026-10-05T02:07:09.745266Z TRACE rbl_dbserver::net: database message sent peer=192.168.1.35:51596 tx=688 kind=menu item args=[0, 233, 2, "", 2, "", 11, 0, 0, 0, 0, 0, 0, 2, "", 0] len=122
2026-10-05T02:07:09.745290Z TRACE rbl_dbserver::net: database message sent peer=192.168.1.35:51596 tx=688 kind=menu item args=[0, 13200, 2, "", 2, "", 13, 0, 0, 0, 0, 0, 0, 2, "", 0] len=122
2026-10-05T02:07:09.745313Z TRACE rbl_dbserver::net: database message sent peer=192.168.1.35:51596 tx=688 kind=menu item args=[1, 1204703889, 6, "Em", 2, "", 15, 0, 0, 0, 0, 0, 0, 2, "", 0] len=126
2026-10-05T02:07:09.745331Z TRACE rbl_dbserver::net: database message sent peer=192.168.1.35:51596 tx=688 kind=menu item args=[0, 5, 2, "", 2, "", 10, 0, 0, 0, 0, 0, 0, 2, "", 0] len=122
2026-10-05T02:07:09.745350Z TRACE rbl_dbserver::net: database message sent peer=192.168.1.35:51596 tx=688 kind=menu item args=[0, 0, 2, "", 2, "", 19, 0, 0, 0, 0, 0, 0, 2, "", 0] len=122
2026-10-05T02:07:09.745378Z TRACE rbl_dbserver::net: database message sent peer=192.168.1.35:51596 tx=688 kind=menu item args=[0, 0, 2, "", 2, "", 6, 0, 0, 0, 0, 0, 0, 2, "", 0] len=122
2026-10-05T02:07:09.745416Z TRACE rbl_dbserver::net: database message sent peer=192.168.1.35:51596 tx=688 kind=menu item args=[1, 267359040, 22, "2026-08-21", 2, "", 46, 0, 0, 0, 0, 0, 0, 2, "", 0] len=142
2026-10-05T02:07:09.745465Z TRACE rbl_dbserver::net: database message sent peer=192.168.1.35:51596 tx=688 kind=menu item args=[0, 267359040, 42, "Delivered By Inflyte", 2, "", 35, 0, 0, 0, 0, 0, 0, 2, "", 0] len=162
2026-10-05T02:07:09.745501Z TRACE rbl_dbserver::net: database message sent peer=192.168.1.35:51596 tx=688 kind=menu item args=[0, 0, 2, "", 2, "", 17, 0, 0, 0, 0, 0, 0, 2, "", 0] len=122
2026-10-05T02:07:09.745524Z TRACE rbl_dbserver::net: database message sent peer=192.168.1.35:51596 tx=688 kind=menu item args=[0, 320, 2, "", 2, "", 16, 0, 0, 0, 0, 0, 0, 2, "", 0] len=122
2026-10-05T02:07:09.745549Z TRACE rbl_dbserver::net: database message sent peer=192.168.1.35:51596 tx=688 kind=menu item args=[0, 1142178522, 24, "Future Rave", 2, "", 14, 0, 0, 0, 0, 0, 0, 2, "", 0] len=144
2026-10-05T02:07:09.745569Z TRACE rbl_dbserver::net: database message sent peer=192.168.1.35:51596 tx=688 kind=menu item args=[0, 0, 2, "", 2, "", 40, 0, 0, 0, 0, 0, 0, 2, "", 0] len=122
2026-10-05T02:07:09.745591Z TRACE rbl_dbserver::net: database message sent peer=192.168.1.35:51596 tx=688 kind=menu item args=[0, 0, 2, "", 2, "", 41, 0, 0, 0, 0, 0, 0, 2, "", 0] len=122
2026-10-05T02:07:09.745622Z TRACE rbl_dbserver::net: database message sent peer=192.168.1.35:51596 tx=688 kind=menu footer args=[] len=20
2026-10-05T02:07:10.236850Z TRACE rbl_dbserver::net: database bytes received peer=192.168.1.35:51596 len=32
2026-10-05T02:07:10.236880Z TRACE rbl_dbserver::net: database request peer=192.168.1.35:51596 tx=689 kind=track info args=[34079745, 267359040]
2026-10-05T02:07:10.237189Z TRACE rbl_dbserver::net: database reply peer=192.168.1.35:51596 tx=689 replies=1
2026-10-05T02:07:10.237203Z TRACE rbl_dbserver::net: database message sent peer=192.168.1.35:51596 tx=689 kind=menu header args=[8450, 7] len=32
2026-10-05T02:07:10.239266Z TRACE rbl_dbserver::net: database bytes received peer=192.168.1.35:51596 len=68
2026-10-05T02:07:10.239284Z TRACE rbl_dbserver::net: database request peer=192.168.1.35:51596 tx=690 kind=render args=[34079745, 0, 7, 0, 7, 12, 1, 0]
2026-10-05T02:07:10.239302Z TRACE rbl_dbserver::net: database reply peer=192.168.1.35:51596 tx=690 replies=9
2026-10-05T02:07:10.239345Z TRACE rbl_dbserver::net: database message sent peer=192.168.1.35:51596 tx=690 kind=render header args=[1, 0] len=32
2026-10-05T02:07:10.239380Z TRACE rbl_dbserver::net: database message sent peer=192.168.1.35:51596 tx=690 kind=menu item args=[1, 1, 2, "", 2, "", 8964, 0, 0, 0, 256, 17, 1204703889, 6, "Em", 13200] len=126
2026-10-05T02:07:10.239430Z TRACE rbl_dbserver::net: database message sent peer=192.168.1.35:51596 tx=690 kind=menu item args=[0, 233, 2, "", 2, "", 11, 0, 0, 0, 0, 0, 0, 2, "", 0] len=122
2026-10-05T02:07:10.239455Z TRACE rbl_dbserver::net: database message sent peer=192.168.1.35:51596 tx=690 kind=menu item args=[0, 13200, 2, "", 2, "", 13, 0, 0, 0, 0, 0, 0, 2, "", 0] len=122
2026-10-05T02:07:10.239479Z TRACE rbl_dbserver::net: database message sent peer=192.168.1.35:51596 tx=690 kind=menu item args=[0, 267359040, 42, "Delivered By Inflyte", 2, "", 35, 0, 0, 0, 0, 0, 0, 2, "", 0] len=162
2026-10-05T02:07:10.239532Z TRACE rbl_dbserver::net: database message sent peer=192.168.1.35:51596 tx=690 kind=menu item args=[9395007, 267359040, 174, "/Volumes/SD/RB/_2026-08-dance/MORTEN & David Guetta - La Révolution (Extended Mix).mp3", 2, "", 0, 0, 0, 0, 0, 0, 0, 2, "", 0] len=294
2026-10-05T02:07:10.239558Z TRACE rbl_dbserver::net: database message sent peer=192.168.1.35:51596 tx=690 kind=menu item args=[0, 1, 2, "", 2, "", 47, 0, 0, 0, 0, 0, 0, 2, "", 0] len=122
2026-10-05T02:07:10.239595Z TRACE rbl_dbserver::net: database message sent peer=192.168.1.35:51596 tx=690 kind=menu item args=[0, 1204703889, 6, "Em", 2, "", 15, 0, 0, 0, 0, 0, 0, 2, "", 0] len=126
2026-10-05T02:07:10.239622Z TRACE rbl_dbserver::net: database message sent peer=192.168.1.35:51596 tx=690 kind=menu footer args=[] len=20
2026-10-05T02:07:10.247865Z TRACE rbl_dbserver::net: database bytes received peer=192.168.1.35:51596 len=32
2026-10-05T02:07:10.247895Z TRACE rbl_dbserver::net: database request peer=192.168.1.35:51596 tx=691 kind=cues args=[34079745, 267359040]
2026-10-05T02:07:10.247912Z TRACE rbl_dbserver::net: database reply peer=192.168.1.35:51596 tx=691 replies=1
2026-10-05T02:07:10.247924Z TRACE rbl_dbserver::net: database message sent peer=192.168.1.35:51596 tx=691 kind=cues reply args=[9476, 0, 1604, <1604 bytes>] len=1648
2026-10-05T02:07:10.547316Z DEBUG rbl_dbserver::net: player closed its database session peer=192.168.1.35:51596
2026-10-05T02:07:14.996920Z DEBUG rbl_link::beacon: peers timed out of the keep-alive table expired=1
2026-10-05T02:07:15.133699Z DEBUG rbl_link::watch: devices silent too long; gone from the network expired=1
2026-10-05T02:07:16.413401Z  INFO rbl_link::beacon: device silent for 6 s; gone from the link number=2 name=CDJ-3000 address=192.168.1.35
2026-10-05T02:07:26.712212Z  INFO rbl_link::beacon: new device on the link number=25 name=NXS-GW kind=Other(8) ip=192.168.1.35 mac=24 97 ed 23 10 72
2026-10-05T02:07:26.712241Z DEBUG rbl_link::watch: device heard on the network number=25 name=NXS-GW kind=Other(8) ip=192.168.1.35
2026-10-05T02:07:32.104742Z DEBUG rbl_link::beacon: device identity query; answering with ours player=192.168.1.35
2026-10-05T02:07:32.184728Z DEBUG rbl_link::beacon: first status from a player; greeting it player=192.168.1.35
2026-10-05T02:07:32.184838Z DEBUG rbl_link::beacon: player listed from its status number=2 name=CDJ-3000 address=192.168.1.35
2026-10-05T02:07:33.072651Z DEBUG rbl_link::beacon: media query about our slot; answering from=192.168.1.35 slot=4 tracks=38740 playlists=680
2026-10-05T02:07:33.075579Z DEBUG rbl_link::beacon: link handshake; answering player=192.168.1.35
2026-10-05T02:07:33.076150Z TRACE rbl_dbserver::net: connection accepted peer=192.168.1.35:43776
2026-10-05T02:07:33.076590Z DEBUG rbl_dbserver::net: port query answered peer=192.168.1.35:43776 port=50278
2026-10-05T02:07:33.078159Z TRACE rbl_dbserver::net: connection accepted peer=192.168.1.35:45116
2026-10-05T02:07:33.078263Z DEBUG rbl_dbserver::net: player connected to the database server peer=192.168.1.35:45116
2026-10-05T02:07:33.078511Z DEBUG rbl_dbserver::net: database greeting exchanged peer=192.168.1.35:45116
2026-10-05T02:07:33.103292Z  INFO rbl_link::beacon: new device on the link number=2 name=CDJ-3000 kind=Cdj ip=192.168.1.35 mac=24 97 ed 23 10 72
```

## What the reboot evidence establishes

- [OBS] The database TCP session closed 300 ms after the final cue reply.
- [OBS] Both the announce watcher and beacon keep-alive table stopped hearing
  the device.
- [OBS] Six seconds after the last traffic, RBX removed CDJ device 2.
- [OBS] Sixteen seconds after the socket closed, the same IP and MAC announced
  as `NXS-GW`, the gateway identity observed during CDJ startup.
- [OBS] Twenty-three seconds after the socket closed, the same physical device
  completed a new database handshake and reappeared as CDJ-3000 device 2.
- [INFER] This is a reboot sequence, not merely RBX losing a database socket or
  a brief Ethernet interruption.
- [OBS] RBX itself stayed running throughout the event and accepted the new
  session. No RBX panic, error, malformed-request warning, or server restart was
  logged around the crash.

## The final cue reply

The last message before the socket closed was the plain cue-list response:

```text
kind=cues reply args=[9476, 0, 1604, <1604 bytes>] len=1648
```

RBX currently produces this as a fixed 1,604-byte all-zero buffer. The code
documents it as the legacy 44-by-36-byte cue slots plus a 20-byte tail, based on
a captured rekordbox response for tracks without old-format cues. The CDJ is
expected to obtain modern cues separately through the extended cue protocol.

Important counter-evidence: five seconds earlier, the same player received an
identically sized cue reply for track `20998959` and continued operating:

```text
2026-10-05T02:07:05.776984Z TRACE rbl_dbserver::net: database request peer=192.168.1.35:51596 tx=676 kind=track info args=[34079745, 20998959]
2026-10-05T02:07:05.792228Z TRACE rbl_dbserver::net: database request peer=192.168.1.35:51596 tx=678 kind=cues args=[34079745, 20998959]
2026-10-05T02:07:05.792308Z TRACE rbl_dbserver::net: database message sent peer=192.168.1.35:51596 tx=678 kind=cues reply args=[9476, 0, 1604, <1604 bytes>] len=1648
```

Therefore the cue reply is the final packet and a prime item to compare byte
for byte, but the log does **not** prove that this packet alone caused the
crash.

## Second disappearance at 20:05 PDT

The same CDJ disappeared again at `20:05:58 PDT`
(`2026-10-05T03:05:58Z`). This followed an even denser peak of 156 database
requests in the single second `03:05:47Z`.

The activity included category navigation, metadata, artwork, `.2EX` and `.EXT`
analysis tags, waveform previews, the key menu, and related-key queries. The
last successfully completed operation was a related-key render:

```text
2026-10-05T03:05:47.800629Z TRACE rbl_dbserver::net: database request peer=192.168.1.35:45116 tx=7089 kind=waveform preview args=[34079745, 0, 268379760, 0, <0 bytes>]
2026-10-05T03:05:47.800653Z TRACE rbl_dbserver::net: database message sent peer=192.168.1.35:45116 tx=7089 kind=waveform preview reply args=[8196, 50, 0, <0 bytes>] len=39
2026-10-05T03:05:47.807663Z TRACE rbl_dbserver::net: database request peer=192.168.1.35:45116 tx=7090 kind=waveform preview args=[34079745, 0, 268380449, 0, <0 bytes>]
2026-10-05T03:05:47.807689Z TRACE rbl_dbserver::net: database message sent peer=192.168.1.35:45116 tx=7090 kind=waveform preview reply args=[8196, 50, 0, <0 bytes>] len=39
2026-10-05T03:05:47.812740Z TRACE rbl_dbserver::net: database request peer=192.168.1.35:45116 tx=7091 kind=waveform preview args=[34079745, 0, 256995314, 0, <0 bytes>]
2026-10-05T03:05:47.812785Z TRACE rbl_dbserver::net: database message sent peer=192.168.1.35:45116 tx=7091 kind=waveform preview reply args=[8196, 50, 0, <0 bytes>] len=39
2026-10-05T03:05:47.818099Z TRACE rbl_dbserver::net: database request peer=192.168.1.35:45116 tx=7092 kind=waveform preview args=[34079745, 0, 256995317, 0, <0 bytes>]
2026-10-05T03:05:47.818121Z TRACE rbl_dbserver::net: database message sent peer=192.168.1.35:45116 tx=7092 kind=waveform preview reply args=[8196, 50, 0, <0 bytes>] len=39
2026-10-05T03:05:48.048588Z TRACE rbl_dbserver::net: database request peer=192.168.1.35:45116 tx=7093 kind=key menu args=[33620993, 0]
2026-10-05T03:05:48.049036Z TRACE rbl_dbserver::net: database message sent peer=192.168.1.35:45116 tx=7093 kind=menu header args=[4116, 24] len=32
2026-10-05T03:05:48.055122Z TRACE rbl_dbserver::net: database request peer=192.168.1.35:45116 tx=7094 kind=render args=[33620993, 0, 24, 0, 24, 12, 1, 0]
2026-10-05T03:05:48.056421Z TRACE rbl_dbserver::net: database message sent peer=192.168.1.35:45116 tx=7094 kind=menu footer args=[] len=20
2026-10-05T03:05:48.071589Z TRACE rbl_dbserver::net: database request peer=192.168.1.35:45116 tx=7095 kind=related keys args=[33686529, 0, 1]
2026-10-05T03:05:48.071631Z TRACE rbl_dbserver::net: database message sent peer=192.168.1.35:45116 tx=7095 kind=menu header args=[4372, 3] len=32
2026-10-05T03:05:48.074318Z TRACE rbl_dbserver::net: database request peer=192.168.1.35:45116 tx=7096 kind=render args=[33686529, 0, 3, 0, 3, 12, 1, 0]
2026-10-05T03:05:48.074706Z TRACE rbl_dbserver::net: database message sent peer=192.168.1.35:45116 tx=7096 kind=menu footer args=[] len=20
2026-10-05T03:05:57.788523Z DEBUG rbl_link::watch: devices silent too long; gone from the network expired=1
2026-10-05T03:05:58.228372Z DEBUG rbl_link::beacon: peers timed out of the keep-alive table expired=1
2026-10-05T03:05:58.832842Z  INFO rbl_link::beacon: device silent for 6 s; gone from the link number=2 name=CDJ-3000 address=192.168.1.35
2026-10-05T03:06:38.390040Z DEBUG rbl_dbserver::net: player closed its database session peer=192.168.1.35:45116
```

[UNKNOWN] No subsequent `NXS-GW` or CDJ-3000 rejoin appears in the inspected
log after this second disappearance. It is therefore evidence that the player
left the network, but not by itself proof of a second reboot.

## Current diagnosis

### Established

1. The primary event was a real reboot of the physical CDJ at
   `192.168.1.35`.
2. RBX did not crash. It continued serving and accepted the CDJ after reboot.
3. The reboot followed an extreme, CDJ-originated burst of artwork, analysis,
   metadata, and render requests.
4. RBX answered those requests with very low latency, allowing more than 100
   requests per second and hundreds of response messages per second.
5. The final response was a successful 1,604-byte plain cue-list response for
   `La Révolution (Extended Mix)`.
6. The same cue response shape had succeeded for another track seconds earlier.
7. The same player later disappeared after another, even denser request burst.

### Leading hypothesis

[INFER] The most likely failure class is a CDJ firmware failure exposed by the
combination of rapid browsing and RBX's essentially unpaced responses. Possible
mechanisms include:

- exhaustion or corruption of a CDJ-side artwork/analysis/browser work queue;
- a race exposed because RBX completes requests much faster than rekordbox;
- a parser fault in one track-specific artwork, `.2EX`, metadata, path, or cue
  response that becomes reachable during the burst;
- an interaction between the accumulated burst and the final cue response.

Response pacing/backpressure is therefore a strong mitigation candidate, but
it is not yet a proven root-cause fix.

### Not established

- [UNKNOWN] Which exact packet or response field caused the firmware failure.
- [UNKNOWN] Whether any response was malformed relative to rekordbox for the
  exact same track.
- [UNKNOWN] Whether the final cue response was causal or merely the last
  completed request before an already-failing process exited.
- [UNKNOWN] Whether the decomposed accent in `La Révolution`, its artwork, its
  `.2EX` data, or its source path contributed.
- [UNKNOWN] The precise physical action at the moment of failure: highlight,
  touch scroll, cue preview, or load.
- [UNKNOWN] The physical CDJ's firmware version. The log identifies the model
  but does not record firmware version.
- [UNKNOWN] The exact commit used to build the running app. `29dc378` is the
  repository HEAD at investigation time, not proof of the live binary's source.
- [UNKNOWN] Whether Ethernet transport contributed. The reboot identity makes
  a simple cable drop insufficient to explain the primary event, but a packet
  capture is needed to rule out retransmission or framing behavior.
- [UNKNOWN] Whether the second disappearance was another reboot.

## CDJ-3000 firmware-emulator reproduction

Test date: 2026-10-04 PDT  
Firmware: CDJ-3000 3.20 / EP122 build 14970 in AtEmu 2.0.0  
RBX source: `29dc378`  
Evidence: `../rbxport-private/verification/e2e-link/incident-20261004/`

The emulator was connected to a consistent 1.7 GB snapshot of the installed
38,740-track database. The snapshot's `share` directory linked to the original
analysis and artwork tree so the firmware received the exact incident
artifacts. RBX ran with `RB_LITE_TEST=1`, and the generated options still
classified the snapshot as a real install, so the database write guard would
refuse any attempted write. No test action edited the installed library.

### Reproduced

- [OBS] Searching for `MORTEN` displayed content ID `267359040`, including the
  decomposed title, original artwork, waveform preview, `Delivered By Inflyte`
  label, 132.0 BPM, and Em key.
- [OBS] Repeated single-detent selector input reproduced the request-rate
  condition. A clean emulator boot reached 125 database requests in one UTC
  second, exceeding the physical incident's 114-request peak. RBX sent 339
  messages totaling 327,896 bytes in that second, including 44 artwork and 45
  analysis-tag replies.
- [OBS] The emulator stayed alive, retained its database session, and remained
  listed as a CDJ-3000 after the burst. There was no `NXS-GW` startup identity,
  database disconnect, or reboot.
- [OBS] Selecting the incident track after the burst succeeded. On the first
  emulator boot, pressing LOAD made firmware 3.20 accept the exact track-info,
  992-byte extended-cue, 8,260-byte beat-grid, 1,604-byte plain-cue, 3,624-byte
  `PWV6`, 105,180-byte `PWV7`, 70,128-byte `PWV5`, 35,070-byte waveform-detail,
  and other analysis replies without crashing or closing the session.
- [OBS] On a second clean boot, another burst peaked at 125 requests/second;
  previewing `La Révolution` afterwards also left the panel, database session,
  and Pro DJ Link identity alive.
- [OBS] Full playback was not validated. The emulated deck reached the deck
  screen with the correct title, artwork, duration, BPM, and key, then displayed
  `E-8305: UNSUPPORTED FILE FORMAT` for the MP3 in this emulator environment.
  This is a load/playback coverage gap, not evidence of the physical reboot.

### Confirmed Link Export implementation bug: `0x0001` is answered

[OBS] Both the physical player and the emulator sent zero-argument message
type `0x0001` while browsing. The local Dysentery protocol reference labels
this message `invalid data`; it is absent from firmware 3.20's named command
table. Several physical occurrences reused the transaction number of a reply
the CDJ had just received. For example, transaction 642 received RBX's metadata
menu header and the CDJ immediately returned `0x0001` on transaction 642.

[OBS] RBX has no message-kind constant or dispatch case for `0x0001`.
`LinkSession::handle` routes every otherwise-unrecognized kind to
`handle_menu`; that function's catch-all creates `Menu::Empty`, and `menu`
answers with a `0x4000` menu header. For `0x0001`, the resulting arguments are
`[1, 0]`. This behavior is in `crates/rbl-dbserver/src/session.rs`: the generic
dispatch is at line 1276, the empty-menu fallback at line 928, and the reply is
built by `menu` at lines 145-154.

[OBS] This incorrect branch was exercised in both environments. The affected
physical CDJ sent four `0x0001` messages in the inspected session; the other
physical CDJ sent one. The clean emulator stress run sent 44, and RBX replied
to every one with the unsolicited menu header.

[OBS] Replying to an invalid-data indication as though it were a menu request
is therefore a confirmed Link Export implementation bug: it adds a reply to a
transaction the player has already rejected.

[INFER] `0x0001` should be consumed without replying. This report does not
implement that change. The extra reply may worsen browser ticket/queue state
under load. The emulator's survival after 44 occurrences proves that this
behavior alone is not sufficient to reboot firmware 3.20.

### Diagnosis after reproduction

The emulator results make a single malformed incident-track field, artwork,
`PWV6`, or fixed plain-cue blob substantially less likely: firmware 3.20 parsed
and rendered those exact artifacts after an equal or heavier request burst.
They do not rule out a physical-only timing, memory-pressure, firmware-version,
or hardware interaction.

The request storm itself is reproducible and has a straightforward cause. Fast
selector movement makes the CDJ rapidly replace the highlighted row and request
metadata, artwork, and `PWV6` preview data for newly visible tracks. RBX replies
in sub-millisecond time, so the firmware can advance this loop at more than 100
requests per second. During that churn the player also rejects some replies with
`0x0001`, while RBX sends an additional response to each rejection.

[INFER] The leading failure mechanism is therefore a CDJ-side asynchronous
browser-ticket or preview-queue race/resource failure exposed by fast scrolling
and unusually fast replies, potentially aggravated by RBX's incorrect response
to `0x0001`. The final 1,604-byte cue reply is more likely the last successful
operation before the failing process exited than a sufficient crash packet.
The physical deck's firmware version remains essential: this reproduction is
only authoritative for firmware 3.20 under AtEmu's timing and memory model.

## Safety assessment

Until this is reproduced and bounded, physical CDJ testing should not use
unrestricted fast browsing against RBX LINK. A hard reboot during live playback
is unacceptable even if the firmware owns the final fault.

No protocol or application behavior has been changed as part of this incident
write-up. The reproduction used a temporary read-only snapshot helper so the
installed rekordbox database was never exposed to writes; that helper was not
retained as application code.

## Required next evidence

1. Preserve the original daily log before the five-file rotation removes it.
2. Capture physical-device traffic for UDP 50000/50002, TCP 12523 and the
   returned database port, NFS/UDP 2049, and mountd during a controlled retry.
3. Compare RBX and rekordbox replies byte-for-byte for content ID `267359040`,
   especially:
   - metadata and track-info menu items;
   - artwork;
   - `.2EX` analysis tag;
   - plain and extended cue lists;
   - waveform preview/detail;
   - the NFS path and audio read sequence if a load occurs.
4. Repeat controlled cases separately:
   - select `La Révolution` without fast scrolling;
   - perform the same fast scroll but stop on another track;
   - request artwork only;
   - request `.2EX` only;
   - request the plain cue list only;
   - replay the recorded request order with and without response pacing.
5. Add per-peer request-rate and in-flight-response telemetry so future logs
   retain queue depth, response latency, and bytes per request class.
6. Test a conservative per-peer pacing/backpressure limit in the emulator
   before trying it on physical hardware.
7. Record the physical CDJ firmware version and obtain any device-side crash or
   diagnostic record available after reboot.

## Commands used to derive the incident counts

These commands are recorded so the totals can be independently checked while
the source log still exists.

```sh
log="$HOME/Library/Application Support/rbxport/logs/rbxport.2026-10-05.log"

# Device departure and restart markers.
rg -n "device silent for 6 s|device said goodbye|player closed its database session|new device on the link" "$log"

# Requests per peer per UTC second.
perl -ne 'if (/^(\d{4}-\d\d-\d\dT\d\d:\d\d:\d\d).*database request peer=([^: ]+)/) {$c{"$1 $2"}++} END {for $k (keys %c) {print "$c{$k}\t$k\n"}}' "$log" | sort -nr

# Densest four-second burst.
perl -ne 'if (/^(\S+)/ && $1 ge "2026-10-05T02:07:00" && $1 le "2026-10-05T02:07:03.999999Z") { $req++ if /database request/; if (/database message sent.* len=(\d+)/) {$bytes += $1; $msg++} $art++ if /kind=artwork reply/; $tag++ if /kind=anlz tag reply/ } END { printf "requests=%d sent_messages=%d sent_bytes=%d artwork_replies=%d analysis_tag_replies=%d\n",$req,$msg,$bytes,$art,$tag }' "$log"

# Full final window through the database socket close.
perl -ne 'if (/^(\S+)/ && $1 ge "2026-10-05T02:07:00" && $1 le "2026-10-05T02:07:10.547316Z") { $req++ if /database request/; if (/database message sent.* len=(\d+)/) {$bytes += $1; $msg++} } END { printf "full_window_requests=%d full_window_sent_messages=%d full_window_sent_bytes=%d\n",$req,$msg,$bytes }' "$log"
```

Expected totals:

```text
requests=269 sent_messages=903 sent_bytes=1778418 artwork_replies=99 analysis_tag_replies=99
full_window_requests=291 full_window_sent_messages=1077 full_window_sent_bytes=1802640
```
