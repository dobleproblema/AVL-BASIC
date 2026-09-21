10 REM Automatic ticks, adaptive precision and optional automatic orientation
20 SCREEN : MODE 640 : SMALLFONT : PAPER 0 : INK 5 : CLG
30 LOCATE 1,0 : GPRINT "1. Dense ticks and crossing - press a key"
40 SCALE -1,1,-3/4,3/4,20 : CROSSAT -0.1,0.3
50 XAXIS 0.1,,,,,2 : YAXIS 0.1,,,,2
60 FRAME : PAUSE
70 CLG : LOCATE 1,0 : GPRINT "2. Thousandths with a fractional crossing - press a key"
80 SCALE -0.005,0.005,-1,1,40 : CROSSAT 0.00025,-0.3
90 XAXIS 0.001 : YAXIS 0.25
100 FRAME : PAUSE
110 CLG : LOCATE 1,0 : GPRINT "3. Long values, automatic orientation - press a key"
120 SCALE 12345.6789,12345.6809,-1,1,40 : CROSSAT 12345.6799,0.4
130 XAXIS 0.0001,,,,2 : YAXIS 0.25
140 FRAME : PAUSE
150 CLG : LOCATE 1,0 : GPRINT "4. Automatic steps: small X, large Y - press a key"
160 SCALE -0.004,0.006,-50000,150000,60 : CROSSAT 0,0
170 XAXIS : YAXIS ,,,,2
180 FRAME : PAUSE
190 CLG : LOCATE 1,0 : GPRINT "5. Dense steps: auto X orientation, unlabeled Y - press a key"
200 SCALE -1,1,-3/4,3/4,20 : CROSSAT -0.1,0.3 'Reduced ticks stay anchored at the crossing
210 XAXIS 0.01,,,,2 : YAXIS 0.01,,,-1
220 FRAME : PAUSE
