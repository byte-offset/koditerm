# koditerm — Feature Ideas

## Browsing / Navigation
- Artist → albums → songs drill-down (library is currently a flat filtered list)
- "Jump to current" — from Now Playing, jump to the playing song/album/artist in the library
- Recently added / recently played views (Kodi exposes these via AudioLibrary)
- Genre browsing
- Saved playlists — load/save from Kodi's playlist library

## Playback
- Seek — `Player.Seek` for remote; `try_seek()` for local (noted in CLAUDE.md)
- Shuffle mode — Kodi has `Player.SetShuffle`
- Queue reordering and item removal

## Display
- Album art — possible in terminals supporting the Kitty graphics protocol or sixels
- Lyrics — Kodi can fetch via `AudioLibrary.GetSongDetails` with the `lyrics` property

## Scrobbling
- Last.fm scrobbling — directly or via Kodi's built-in Last.fm addon

## MusicBrainz Integration
- Match library tracks/artists to MBIDs via `AudioLibrary.GetArtistDetails`
  (`musicbrainzartistid` property — Kodi often stores these)
- Fetch extended metadata: release info, tags, aliases, genres
- **Wander / discovery mode:**
  1. Get current artist's MBID from Kodi
  2. Query MB API for artist relationships (member-of, collaborated-with, influenced-by)
  3. Walk the graph, filtering to artists present in the local Kodi library
  4. Play current artist → pick a collaborator in library → play them → repeat
  - Genuinely novel feature for a terminal player
  - Could display the "connection" between artists as it wanders

## Other
- Ratings input (Kodi supports storing ratings)
- Play count and last-played date display in track info
- Multiple queue management
