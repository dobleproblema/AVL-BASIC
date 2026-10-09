100 REM A short PNG dance loop. Press ESC to exit.
105 REM Load every frame once; PlaybackFps sets the animation speed.
110 SCREEN : MODE 640
120 FrameCount=8 : PlaybackFps=6
130 DIM Picture$(FrameCount-1)
140 FOR FrameIndex=0 TO FrameCount-1
150   BLOAD "assets/anime-dance/frame"+STR$(FrameIndex)+".png",Picture$(FrameIndex)
160 NEXT FrameIndex
170 FrameIndex=0
180 SCREEN Picture$(FrameIndex)
190 FRAME PlaybackFps
200 IF INKEY$=CHR$(27) THEN SCREEN CLOSE:END
210 FrameIndex=(FrameIndex+1) MOD FrameCount
220 GOTO 180
