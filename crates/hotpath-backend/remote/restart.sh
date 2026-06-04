#!/bin/bash
ssh $TARGET_NODE << EOF
for session in \$(screen -ls | grep "hotpath" | awk '{print \$1}'); do
    echo "Terminating session \$session"
    screen -X -S "\$session" quit
done
EOF

ssh "$TARGET_NODE" << 'EOF'
screen -d -m -S hotpath bash -c 'cd /root/hotpath-backend && set -a && . ./.env && set +a && ./server > dbg.log 2>&1'
EOF
