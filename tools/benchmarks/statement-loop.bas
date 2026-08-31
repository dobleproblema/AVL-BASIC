10 REM Tight statement-loop benchmark for interpreter overhead
20 REM Use an optimized build and compare the median of several runs.
30 N=10000000
40 A=0
50 T=TIME
60 FOR I=1 TO N
70 A=A+I
80 NEXT I
90 E=TIME-T
100 PRINT "ITERATIONS:";N
110 PRINT "SECONDS:";E
120 PRINT "NS/ITERATION:";E*1000000000/N
130 END
