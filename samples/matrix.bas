10 MAT BASE 1
15 DIM a(3,3)
20 MAT READ a
25 MAT b=INV(a)
30 MAT c=a*b
35 MAT d=TRN(a)
40 MAT PRINT USING "##.###";a;b;c;d
45 PRINT "Determinant=";DET(a)
50 DATA 5, 3, 1, 3, 7, 4, 1, 4, 9
55 END
