10 MODE 640
15 f=36
20 DIM gr$(f)
25 FOR x=0 TO f
30 BLOAD "assets/z-3dplot"+STR$(x)+".png",gr$(x)
35 NEXT
40 i=0
45 t=TIME
50 FOR c=1 TO 1000
55 SCREEN gr$(i)
60 FRAME 60
65 i=(i+1) MOD (f+1)
70 NEXT
75 PRINT "Elapsed time:";TIME-t

