PORTNAME=	netman
DISTVERSIONPREFIX=	v
DISTVERSION=	1.0.0
CATEGORIES=	net-mgmt

MAINTAINER=	fbsd.bugzilla@fenyo.net
COMMENT=	Passive network monitor with side-by-side layer 2 and 3 views
WWW=		https://github.com/AlexandreFenyo/netman-reborn

LICENSE=	MIT
LICENSE_FILE=	${WRKSRC}/LICENSE

USES=		cargo
USE_GITHUB=	yes
GH_ACCOUNT=	AlexandreFenyo
GH_PROJECT=	netman-reborn

USE_RC_SUBR=	${PORTNAME}

SUB_FILES=	pkg-message

PLIST_FILES=	bin/${PORTNAME} \
		share/man/man1/${PORTNAME}.1.gz
PORTDATA=	*
PORTDOCS=	README.md RESEARCH.md

OPTIONS_DEFINE=	DOCS

# Upstream serves the frontend from the directory given by --static-dir,
# whose default is relative to the current directory.  Point it at the
# installed copy so netman works from anywhere, and expand the same paths in
# the FreeBSD-specific parts patched into the manual page.
post-patch:
	@${REINPLACE_CMD} -e 's|default_value = "static"|default_value = "${DATADIR}/static"|' \
		${WRKSRC}/src/main.rs
	@${REINPLACE_CMD} -e 's|%%PREFIX%%|${PREFIX}|g' -e 's|%%DATADIR%%|${DATADIR}|g' \
		${WRKSRC}/${PORTNAME}.1

post-install:
	${INSTALL_MAN} ${WRKSRC}/${PORTNAME}.1 ${STAGEDIR}${PREFIX}/share/man/man1/
	@${MKDIR} ${STAGEDIR}${DATADIR}/static
	@cd ${WRKSRC}/static && ${COPYTREE_SHARE} . ${STAGEDIR}${DATADIR}/static

post-install-DOCS-on:
	@${MKDIR} ${STAGEDIR}${DOCSDIR}
	${INSTALL_DATA} ${PORTDOCS:S|^|${WRKSRC}/|} ${STAGEDIR}${DOCSDIR}

.include <bsd.port.mk>
