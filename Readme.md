# slpz
Slpz compresses and decompresses Slippi replay (slp) files. 
The slpz format is 100% supported by [rwing](https://x.com/rwing_aitch/status/1844056466283692388), meaning you can watch back, browse, and export savestates even while your replays are compressed.  

You can expect slpz files to be around 8x to 12x times smaller than slp files for regular matches. (~3Mb down to ~300Kb).
On my old thinkpad it can compress around 120 replays per second and decompress around 340 replays per second.

Compression is done with zstd.
zstd is not required on the user's computer.
The library is statically linked at compile time.

Important information, such as player tags, stages, date, characters, etc. all remain uncompressed in the slpz format. 
This allows slp file browsers to easily parse and display this information without needing to decompress the replay.

# The slpz program
You can download slpz [here](https://github.com/AlexanderHarrison/slpz/releases/latest).
This program allows commandline compression and decompression of both files and entire directories.

For example, the command `slpz -r --rm -x ~/Slippi/` will compress every replay in your Slippi replay directory.
The command `slpz -r --rm -d ~/Slippi/` will decompress them.

[Rwing](https://x.com/rwing_aitch/status/1844056466283692388) has a straightfoward process to compress/decompress built-in, 
so you can use rwing if you don't want to use the command line.

Programmers can also use slpz as a [library](https://crates.io/crates/slpz).

# The slpz Format

## Header
24 bytes.
- 0..4: Version. Current version is 0
- 4..8: Event Sizes offset
- 8..12: Game Start offset
- 12..16: Metadata offset
- 16..20: Compressed events offset
- 20..24: Size of uncompressed events

All offsets are from file start. 

The sections must be contiguous. E.x. the metadata section is always from `slpz[metadata_offset..compressed_events_offset]`,
and the compressed events section is from `slpz[compressed_events_offset..]`.

## Event Sizes
This is equivalent to the 'Event Payloads' event in the [SLP Spec](https://github.com/project-slippi/slippi-wiki/blob/master/SPEC.md#event-payloads).

## Game start
This is equivalent to the 'Game Start' event in the [SLP Spec](https://github.com/project-slippi/slippi-wiki/blob/master/SPEC.md#game-start).

## Metadata
This is equivalent to the 'Metadata' event in the [SLP Spec](https://github.com/project-slippi/slippi-wiki/blob/master/SPEC.md#the-metadata-element).

## Compressed Events
This is reordered events passed through zstd compression.

### Event Reordering?
Reordering the bytes in events increases the compression ratio ~2x.

A normal slp file is a stream of events consisting of a command byte and statically sized payload.
Event payloads are almost all the same, so we can reorder the data to increase the compressability of the data.

We first turn the event stream into a list of command bytes, keeping the order but removing the payloads.
Then a list of all of the first bytes in the payloads for events with command 0, 
then all of the second bytes in the payloads for events with command 0,
all the way to a list of the last bytes of the payloads for command 255.

To undo this reordering we also need the number of total events, so we put this in as 4 bytes at the start.

#### Example
```
cmd ABCD cmd2 EFG cmd ABCD cmd3 HI cmd2 EFG
```

converts to:
```
// Event Order
5 cmd cmd2 cmd cmd3 cmd2
// Reordered Event Data
AABBCCDD EEFFGG HI
```

# Comparison with [slippc](https://github.com/pcrain/slippc)
slippc is very impressive. 
They have achieved much higher compression rates by abusing the contents of events.
However, in my opinion, this comes with two big drawbacks:
1. **Maintentance**: Due to abusing the structure of events, slippc is beholden to the slp spec and must be manually updated for each new version.
*Slippc has not been updated for over a year and fails on new replays.*
slpz does not care about the contents of events. (Other than the Event Payloads event). 
It will work for all slp spec changes in the future.
2. **Performance**: slpz uses zstd compression. slippc uses lzma compression.
lzma compresses slightly better than zstd, but takes order of magnitudes longer to compress and decompress.
Incredibly fast decompression allows slpz files to be watched back, browsed, and used just like regular slp files,
without needing seconds of waiting for decompression.
