10 REM HP-85 style copies: blocks, rows, columns and overlapping ranges
20 MAT BASE 1 : DIM A(4,4),B(4,4),V(4)
30 FOR R=1 TO 4 : FOR C=1 TO 4 : A(R,C)=10*R+C : NEXT C : NEXT R
40 PRINT "Original matrix" : MAT PRINT A
50 MAT B=A(2:3,2:4)
60 PRINT "Rows 2..3, columns 2..4" : MAT PRINT B
70 MAT V=A(2,)
80 PRINT "Row 2 copied into a vector" : MAT PRINT ROW V
90 MAT A(,4)=V
100 PRINT "That vector copied into column 4" : MAT PRINT A
110 MAT V(2:4)=V(1:3)
120 PRINT "Overlapping copy uses the original values" : MAT PRINT ROW V
130 REM HP descending ranges exclude their written endpoints: 5:0 means 4,3,2,1.
140 MAT V=V(5:0)
150 PRINT "Reversed vector" : MAT PRINT ROW V
160 END
