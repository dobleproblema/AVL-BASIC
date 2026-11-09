10 MAT BASE 1
15 DIM a(3,3)
20 MAT READ a
25 MAT PRINT COLOR(3);a(1,)
30 MAT PRINT COLOR(2);a(2:UBOUND(a,1),)
35 PRINT
40 FOR r=1 TO UBOUND(a,1) ' Empty side slices add no cells or separators.
45   MAT PRINT a(r,1:r-1) | COLOR(2);a(r,r) | COLOR();a(r,r+1:UBOUND(a,2))
50 NEXT r
55 PRINT
60 PRINT "Same slice, normal and transposed"
65 MAT PRINT USING "0";a(1:2,2:3) | "  |  " | COL a(1:2,2:3) ' COL transposes the slice for display.
70 PRINT
75 MAT PRINT USING "#";a(,1) | "" | a(,2) | "" | a(,3) ' An empty separator joins the blocks directly.
80 PRINT
85 MAT a=TRN(1/a)
90 REM Commas keep both blocks in wide fields.
95 MAT PRINT COLOR(2);a(,1:2), | COLOR(3);a(,3:UBOUND(a,2)),
100 PRINT
105 REM COLOR with one argument keeps the row 's background.
110 MAT PRINT USING "0.000";COLOR(2,5);a(1:1,1) | COLOR(3);a(1:1,2) | COLOR(2);a(1:1,3:UBOUND(a,2))
115 MAT PRINT USING "0.000";a(2:UBOUND(a,1),1) | COLOR(3);a(2:UBOUND(a,1),2) | COLOR();a(2:UBOUND(a,1),3:UBOUND(a,2))
120 DATA 1,2,3,4,5,6,7,8,9
