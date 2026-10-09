10 SCREEN : MODE 640 : PAPER 0 : CLG OFFSCREEN
20 BLOAD "assets/texture.png",a$
25 SPRITE a$,0,0 : SPRITE a$,480,0 : a$=SCREEN$
30 BLOAD "assets/anime.png",b$
40 t=TIME
50 FOR c=1 TO 1000
60 SCREEN a$ : FRAME
70 SWAP a$,b$
80 NEXT
90 PRINT 1000/(TIME-t);" fps"

