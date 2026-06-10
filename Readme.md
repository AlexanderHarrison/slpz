# slpz
Slpz compresses and decompresses Slippi replay (slp) files. 
The slpz format is 100% supported by [rwing](https://x.com/rwing_aitch/status/1844056466283692388), meaning you can watch back, browse, and export savestates even while your replays are compressed.  

You can expect slpz files to be around 8x to 12x times smaller than slp files for regular matches. (~3Mb down to ~300Kb).
On my old thinkpad it can compress around 120 replays per second and decompress around 340 replays per second.

Events are compressed using [zstd](https://github.com/facebook/zstd).
zstd is not required on the user's computer. The library is statically linked at compile time.

The Game Start and Metadata events (player tags, stages, date, characters, etc.) aren't compressed in the slpz format.
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

The slpz format has five contiguous sections: Header, Event Sizes, Game Start, Metadata, and Compressed Events.
Each section past the header has an offset defined in the header.
Sections run from its offset until the next section, or EOF for the compressed events section.
All offsets are from file start.

For example, the metadata section is from `slpz[metadata_offset..compressed_events_offset]`,
and the compressed events section is from `slpz[compressed_events_offset..]`.

## Header
24 bytes.
- 0..4: Version. Current version is 0
- 4..8: Event Sizes offset
- 8..12: Game Start offset
- 12..16: Metadata offset
- 16..20: Compressed events offset
- 20..24: Size of uncompressed events

## Event Sizes, Game Start, Metadata
These sections are the same as the SLP Spec. They are simply copied over from the slp.
- [Event Sizes](https://github.com/project-slippi/slippi-wiki/blob/master/SPEC.md#event-payloads)
- [Game Start](https://github.com/project-slippi/slippi-wiki/blob/master/SPEC.md#game-start)
- [Metadata](https://github.com/project-slippi/slippi-wiki/blob/master/SPEC.md#the-metadata-element)

## Compressed Events
This event contains compressed slp events and is the bulk of an slpz file.
There are two steps to construct this section: byte reordering, then zstd compression.

Running zstd alone on slp events does an OK job, but we can reorder the event data to increase compressability.
The key idea is to place the first byte of the ID 0 events together,
then all of the second bytes of ID 0 events together,
up until the last bytes of the ID 255 events.

To undo this, we store how many events there are and all the event ids in order before the reordered event bytes.

### Reordering Example
```
cmd ABCD cmd2 EFG cmd ABCD cmd3 HI cmd2 EFG
```

converts to:
```
// event count
5
// event id list
cmd cmd2 cmd cmd3 cmd2
// reordered event bytes
AABBCCDD EEFFGG HI
```

Finally, we compress all of this with zstd and place it in the slpz file.

# Comparison with [slippc](https://github.com/pcrain/slippc)
slippc is very impressive. 
They have achieved higher compression rates by abusing the contents of events.
However, this comes with two big drawbacks:
1. **Maintentance**: Due to abusing the structure of events, slippc is beholden to the slp spec and must be manually updated for each new version.
*Slippc has not been updated for over a year and fails on new replays.*
slpz does not care about the contents of events, and will work for all slp spec changes in the future.
2. **Performance**: slpz uses zstd compression. slippc uses lzma compression.
lzma compresses slightly better than zstd, but takes order of magnitudes longer to compress and decompress.
Incredibly fast decompression allows slpz files to be watched back, browsed, and used just like regular slp files,
without needing seconds of waiting for decompression.
