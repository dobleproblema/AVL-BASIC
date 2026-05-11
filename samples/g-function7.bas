100 DEF FNX(u)=(u*(u^2-47^2)*(u^2-88^2)*(u^2-117^2))^2
110 MODE 800
120 B=70 'Border discarded at each end so the labels fit
130 SCREEN : CLG
140 SCALE -128,128,0,1.8E+27,B
150 PENWIDTH 2 : INK 2
160 GRAPH FNX(X)
170 INK 1
180 CROSSAT 256,0 : XAXIS 32 'CROSSAT outside the drawing area so all the labels remain centered
190 CROSSAT -128,0 : YAXIS 1.8E+27 'If ticks use exponential notation, labels do too
200 CROSSAT 128,0 : YAXIS 1E+5 'If ticks are too dense, they are ignored
210 END
