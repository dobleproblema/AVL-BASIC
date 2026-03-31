10 REM Example showing how DEF SUB works with local variables
15 DEF SUB POINT(x,y)
20   LOCAL c(20),r,g,b
25   LOCAL h 'LOCAL declarations must be defined in the lines immediately after DEF SUB
30   r=INT(RND*256) : g=INT(RND*256) : b=INT(RND*256)
35   c(5)=RGB(r,g,b)
40   PLOT x,y,c(5)
45 SUBEND
50 REM Main program
55 CLG : PENWIDTH 4
60 FOR r=1 TO 5
65 PRINT c(5);"(";r;g;b;")"; 'Global values remain unchanged outside POINT
70 CALL POINT(INT(RND*640),INT(RND*480))
75 FRAME : PAUSE 10
80 NEXT r
85 PRINT c(15) 'Fails: the array c(20) was local to the SUB so c(15) is not defined here
