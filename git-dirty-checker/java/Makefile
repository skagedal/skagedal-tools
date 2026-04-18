GRAALVM_HOME=/Library/Java/JavaVirtualMachines/graalvm-25.jdk/Contents/Home

git-dirty-checker: 
	JAVA_HOME=$(GRAALVM_HOME) mvn compile
	JAVA_HOME=$(GRAALVM_HOME) $(GRAALVM_HOME)/bin/native-image -classpath ./target/classes -o git-dirty-checker tech.skagedal.GitDirtyChecker 

test: git-dirty-checker
	./git-dirty-checker --help
	./git-dirty-checker --help | grep -q 'Usage: git-dirty-checker'

install: git-dirty-checker
	cp git-dirty-checker ~/local/bin/git-dirty-checker 

clean:
	mvn clean
	rm -f git-dirty-checker

.PHONY: clean native test
