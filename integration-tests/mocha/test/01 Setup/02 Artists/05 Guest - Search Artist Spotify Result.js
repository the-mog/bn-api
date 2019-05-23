const supertest = require('supertest');
const expect = require('chai').expect;
const mocha = require('mocha');
const tv4 = require('tv4');
const fs = require('fs');
const pm = require('../../pm');const debug=require('debug');var log = debug('bn-api');

const baseUrl = supertest(pm.environment.get('server'));

const apiEndPoint = '/artists/search?q=Powerwolf&spotify=1';


var response;
var responseBody;


const post = async function (request_body) {
    return baseUrl
        .post(pm.substitute(apiEndPoint))
        .set('Accept', 'application/json')
        .set('Content-Type', 'application/json')

        .send(pm.substitute(request_body));
};

const get = async function (request_body) {
    return baseUrl
        .get(pm.substitute(apiEndPoint))


        .set('Accept', 'application/json')
        .send();
};

let requestBody = ``;


describe('Guest - Search Artist Spotify Result', function () {
    before(async function () {
        response = await get(requestBody);
        log(response.request.header);
        log(response.request.url);
        log(response.request._data);
        log(response.request.method);
        responseBody = JSON.stringify(response.body);
        //log(pm);
        log(response.status);
        log(responseBody);
    });

    after(async function () {
        // add after methods


    });

    it("should be 200", function () {
        expect(response.status).to.equal(200);
    })

//If data is blank, no spotify key was provided


    it("Spotify result should contain spotify_id or data should be blank", function () {
        pm.environment.set("last_spotify_artist_id", "");
        let data = JSON.parse(responseBody).data;
        if (data.length === 0) {
            // postman.setNextRequest("Admin - Create Venue - Public");
            log("No artist found");
        } else {
            data.forEach(item => {
                expect(item.spotify_id).to.not.be.a("null");
                pm.environment.set("last_spotify_artist_id", "5HFkc3t0HYETL4JeEbDB1v");
            })
        }

    });


});

            
