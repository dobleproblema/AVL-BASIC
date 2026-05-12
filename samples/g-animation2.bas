10 MODE 640
15 f=12
20 DIM gr$(f)
25 FOR x=0 TO f
30 BLOAD "assets/z-tree"+STR$(x)+".png",gr$(x)
35 NEXT
40 i=0 : k=1
45 t=TIME
50 FOR c=1 TO 1000
55 SCREEN gr$(i)
60 FRAME 60
65 i=(i+k) MOD (f+1)
70 IF i=0 OR i=f THEN k=-k
75 NEXT
80 PRINT "Elapsed time:";TIME-t

