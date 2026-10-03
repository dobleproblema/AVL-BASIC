10 REM IIF evaluates the condition once and only the selected branch.
20 FOR X=0 TO 2
30 PRINT "X="+STR$(X)+" ->"+STR$(IIF(X<>0,10/X,0))
40 NEXT X
50 PRINT IIF(1,"The unused division is safe",1/0)
60 PRINT IIF(0,1/0,IIF(1,"Nested IIF also works",1/0))
70 PRINT IIF(1,"A string result",123)
80 PRINT IIF(0,"Unused string",123)
90 END
