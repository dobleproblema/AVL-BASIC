10 SCREEN : CLG
15 B=80
20 SCALE 1960,1990,0,200,B
25 REM If XAXIS is drawn before CROSSAT, it does not know that
30 REM the Y axis crosses it at 1960, so the crossing label
35 REM ends up centered on its tick. Useful trick when,
40 REM as in this case, we prefer it centered instead of shifted
45 REM to the right to avoid the axis cutting through it (which is
50 REM the standard behavior).
55 XAXIS 10,,,,,10
60 CROSSAT 1960,0
65 YAXIS 30,,,,6
70 PENWIDTH 4 : INK 2
75 PLOT 1975,90
80 END
