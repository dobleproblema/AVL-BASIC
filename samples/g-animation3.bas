10 MODE 640
15 f=35
20 DIM gr$(f)
25 FOR x=0 TO f
30 BLOAD "assets/z-light"+STR$(x)+".png",gr$(x)
35 NEXT
40 i=0
45 t=TIME
50 FOR c=1 TO 1000
55 SCREEN gr$(i)
60 i=(i+1) MOD (f+1)
65 NEXT
70 PRINT "Elapsed time:";TIME-t

