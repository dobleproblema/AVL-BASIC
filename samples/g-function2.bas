100 MODE 1024
110 B=20 'Border wide enough for the rounded axis labels
120 P=0.01 'Increment per step
130 SCREEN : CLG
140 SCALE -PI,PI,-10,10,B
150 PENWIDTH 2 : INK 2
160 GRAPH TAN(X),P
170 INK 1
180 XAXIS ROUND(PI/2,2) 'Rounded spacing gives labels with at most two decimals here
190 YAXIS 3,,,1
200 END
