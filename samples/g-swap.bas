10 MODE 640
20 BLOAD "assets/cloth-poster.png",a$
30 BLOAD "assets/anime.png",b$
40 t=TIME
50 FOR c=1 TO 1000
60 SCREEN a$ : FRAME
70 SWAP a$,b$
80 NEXT
90 PRINT 1000/(TIME-t);" fps"

