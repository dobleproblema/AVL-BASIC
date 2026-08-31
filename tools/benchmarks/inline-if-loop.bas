10 REM Inline IF fast-path benchmark
20 REM Use an optimized build and compare the median of several runs.
30 N=5000000
40 A=0
50 T=TIME
60 FOR I=1 TO N
70 IF I THEN A=A+I ELSE A=A-1
80 NEXT I
90 E=TIME-T
100 PRINT "ITERATIONS:";N
110 PRINT "SECONDS:";E
120 PRINT "NS/ITERATION:";E*1000000000/N
130 END
