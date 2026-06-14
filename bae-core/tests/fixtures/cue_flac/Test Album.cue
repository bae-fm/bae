REM Test CUE/FLAC fixture with pregap
PERFORMER "Test Artist"
TITLE "Test Album"
FILE "Test Album.flac" WAVE
  TRACK 01 AUDIO
    TITLE "Track One (Silence)"
    PERFORMER "Test Artist"
    INDEX 01 00:00:00
  TRACK 02 AUDIO
    TITLE "Track Two (White Noise)"
    PERFORMER "Test Artist"
    INDEX 00 00:08:00
    INDEX 01 00:10:00
  TRACK 03 AUDIO
    TITLE "Track Three (Brown Noise)"
    PERFORMER "Test Artist"
    INDEX 01 00:20:00
