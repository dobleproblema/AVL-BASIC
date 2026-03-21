100 MODE 1024
110 B=20 'Border discarded at each end so, for example, the labels fit
120 P=0.01 'Increment per step
130 SCREEN : CLG
140 SCALE -PI,PI,-10,10,B
150 PENWIDTH 2 : INK 2
160 GRAPH TAN(X),P
170 INK 1
180 XAXIS PI/2
190 YAXIS 3,,,1
200 END

