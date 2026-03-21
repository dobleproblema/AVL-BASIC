100 DEF FNX(X)=1/X 'The one-variable function we want to plot
110 MODE 800
120 B=40 'Border discarded at each end so the labels fit
130 SCREEN : CLG
140 SCALE -10,0,-2,0,B
150 XAXIS 1,,,1
160 YAXIS 0.5,,,1
170 PENWIDTH 2 : INK 2
180 GRAPH FNX(X)
190 END

