local c = true
while c do
    print "hello, while"
    if true then
        c = false
        continue
    end
    print "should not print this!"
end

