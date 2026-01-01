GRAALVM_HOME=/Library/Java/JavaVirtualMachines/graalvm-25.jdk/Contents/Home

git-repos-latest-activity:
	JAVA_HOME=$(GRAALVM_HOME) mvn compile
	JAVA_HOME=$(GRAALVM_HOME) $(GRAALVM_HOME)/bin/native-image -classpath ./target/classes -o git-repos-latest-activity tech.skagedal.GitReposLatestActivity

install: git-repos-latest-activity
	cp git-repos-latest-activity ~/local/bin/git-repos-latest-activity

clean:
	mvn clean
	rm -f git-repos-latest-activity

.PHONY: clean install
