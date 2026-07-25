10 REM Empty modern subroutine CALL microbenchmark
20 REM Use an optimized build and compare the median of several runs.
30 DEF SUB NOP
40 SUBEND
50 N=2000000
60 T=TIME
70 FOR I=1 TO N
80 CALL NOP
90 NEXT I
100 E=TIME-T
110 PRINT "CALLS:";N
120 PRINT "SECONDS:";E
130 PRINT "NS/CALL:";E*1000000000/N
140 END
