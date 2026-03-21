100 DEF FNX(X)=TAN(X) 'The one-variable function we want to plot
101 MODE 1024
105 B=20 'Border discarded at each end so, for example, the labels fit
110 D=-PI : H=PI 'Start and end of the plot; the function may extend beyond the screen
115 P=0.01 'Precision: the larger the number, the lower the precision and the higher the speed
120 SCREEN : CLG
125 SCALE -PI,PI,-10,10,B
130 ON ERROR GOTO 210
135 PENWIDTH 2 : INK 2
140 XP=D : YP=FNX(D)
145 MOVE XP,YP 'Place the first point outside the loop
150 FOR X=D TO H STEP P
155 XA=X : YA=FNX(X)
160 IF ABS(YA-YP)<20 THEN DRAW XA,YA ELSE MOVE XA,YA
165 YP=YA
170 NEXT X
175 INK 1
180 XAXIS PI/2
185 YAXIS 3,-10,10,1
190 END
210 RESUME NEXT

