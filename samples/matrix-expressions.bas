10 REM Compound MAT expressions use BASIC precedence.
20 MAT BASE 0
30 DIM A(1,1),B(1,1),BIAS(1,1)
40 DATA 1,2,3,4,2,0,0,3
50 MAT READ A,B
60 MAT BIAS=1
70 MAT RESULT=BIAS+A*B
80 PRINT "BIAS+A*B: multiply first"
90 MAT PRINT RESULT
100 MAT RESULT=(BIAS+A)*B
110 PRINT "(BIAS+A)*B: add first"
120 MAT PRINT RESULT
130 MAT RESULT=TRN(A+BIAS)*2+ABS(-1)
140 PRINT "Transpose, scale and scalar function in one expression"
150 MAT PRINT RESULT
160 MAT RESULT=INV(A+A)*A
170 PRINT "INV(A+A)*A: half the identity matrix"
180 MAT PRINT RESULT
190 MAT RESULT=A+SUM(A)*2
195 PRINT "A+SUM(A)*2: a matrix statistic supplies a scalar"
197 MAT PRINT RESULT
200 REM ^ keeps BASIC's left associativity; unary - follows ^.
210 MAT RESULT=-A^2
220 PRINT "-A^2: negate the elementwise square"
230 MAT PRINT RESULT
240 REM Subarray selectors stay restricted to copying.
250 MAT TOP=A(0,)
260 MAT SCALED=TOP*2+1
270 PRINT "Copy a row, then scale and offset its values"
280 MAT PRINT SCALED
290 END
