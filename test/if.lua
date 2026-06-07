if a then
    print "skip this"
end
if print then
    local a = "I am true"
    print(a)
end

print(a) -- should be nil

if a then
    print "skip this"
else
    print "else branch"
end
