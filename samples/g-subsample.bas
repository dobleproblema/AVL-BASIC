10 REM Example showing how DEF SUB works with local variables
15 DEF SUB BIGPOINT(x,y)
20   LOCAL c(20)
25   LOCAL r,g,b,c 'LOCAL declarations must be defined in the lines immediately after DEF SUB
30   r=INT(RND*256) : g=INT(RND*256) : b=INT(RND*256)
35   c(5)=RGB(r,g,b) : c=5+RND*10 'Local scalar and array may share a name
40   FCIRCLE x,y,c,c(5)
45 SUBEND
50 REM Main program
55 CLG
60 FOR r=1 TO 5
65 PRINT c(5);"(";r;g;b;c;")"; 'Global values remain unchanged outside BIGPOINT
70 CALL BIGPOINT(INT(RND*640),INT(RND*480))
75 FRAME
80 NEXT r
85 PRINT c(15) 'Fails: the array c(20) was local to the SUB so c(15) is not defined here
